"""Konverter Spiel-Dump -> game_v1 (tools/build_game_v1.py) mit kleinen, selbst gebauten
Modellen (keine Ninja-Kiwi-Daten im Repo)."""
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
import build_game_v1 as b  # noqa: E402


def T(typ, **kw):
    return {"$type": f"Il2CppAssets.Scripts.Models.{typ}, Assembly-CSharp", **kw}


def dart_like(rotate=True, extra_behaviors=(), proj_extra=(), emission=None):
    proj = T("ProjectileModel", pierce=2.0, radius=2.0, filters=[T("FilterInvisibleModel", isActive=True)],
             behaviors=[T("DamageModel", damage=1.0, maxDamage=0.0, immuneBloonProperties=17),
                        T("TravelStraitModel", speed=300.0, lifespan=0.25), *proj_extra])
    attack_behaviors = [T("RotateToTargetModel")] if rotate else []
    attack = T("AttackModel", range=32.0, name="AttackModel_Attack",
               weapons=[T("WeaponModel", rate=0.95, emission=emission or T("SingleEmissionModel"), projectile=proj)],
               behaviors=attack_behaviors)
    return T("TowerModel", cost=200.0, range=32.0, footprint=T("CircleFootprintModel", radius=6.0),
             behaviors=[attack, *extra_behaviors, T("DisplayModel", display="x")])


def test_basic_projectile():
    v = b.convert_variant(dart_like(), [0, 0, 0])
    a = v["attacks"][0]
    assert (v["range"], v["footprint"], a["cooldown"], a["pierce"], a["damage"]) == (32.0, 6.0, 0.95, 2, 1.0)
    assert a["cannot_damage"] == ["lead", "frozen"] and a["camo"] is False
    assert a["delivery"]["kind"] == "projectile" and a["delivery"]["speed"] == 300.0


def test_tower_without_rotate_to_target_shoots_fixed_directions():
    arc = T("ArcEmissionModel", count=8, angle=360.0, offset=0.0, offsetStart=0.0, sliceSize=0.0)
    v = b.convert_variant(dart_like(rotate=False, emission=arc), [0, 0, 0])
    assert v["attacks"][0]["delivery"]["emission"]["fixed"] is True
    v = b.convert_variant(dart_like(rotate=True, emission=arc), [0, 0, 0])
    assert v["attacks"][0]["delivery"]["emission"]["fixed"] is False


def test_frag_shards_inherit_hit_set():
    shard = T("ProjectileModel", pierce=2.0, radius=2.0, filters=[],
              behaviors=[T("DamageModel", damage=1.0, maxDamage=0.0, immuneBloonProperties=17),
                         T("TravelStraitModel", speed=300.0, lifespan=0.17)])
    contact = T("CreateProjectileOnContactModel", projectile=shard, passOnCollidedWith=True,
                emission=T("ArcEmissionModel", count=8, angle=360.0, offset=0.0, useProjectileRotation=False))
    v = b.convert_variant(dart_like(proj_extra=[contact]), [0, 0, 0])
    c = v["attacks"][0]["delivery"]["on_contact"][0]
    assert (c["kind"], c["inherit_hit"], c["emission"]["fixed"]) == ("shards", True, True)


@pytest.mark.parametrize("model, why", [
    (T("KnockbackModel"), "KnockbackModel"),
    (T("AbilityModel"), "AbilityModel"),
])
def test_unknown_gameplay_models_are_refused(model, why):
    with pytest.raises(b.Unsupported, match=why):
        b.convert_variant(dart_like(extra_behaviors=[model]), [0, 0, 0])


def test_gameplay_model_below_cosmetic_node_is_found():
    hidden = T("DisplayModel", behaviors=[T("RetargetOnContactModel")])
    with pytest.raises(b.Unsupported, match="RetargetOnContactModel"):
        b.convert_variant(dart_like(extra_behaviors=[hidden]), [0, 0, 0])


def test_refuses_age_on_straight_projectile_and_max_damage():
    with pytest.raises(b.Unsupported, match="AgeModel"):
        b.convert_variant(dart_like(proj_extra=[T("AgeModel", lifespan=1.0)]), [0, 0, 0])
    m = dart_like()
    m["behaviors"][0]["weapons"][0]["projectile"]["behaviors"][0]["maxDamage"] = 5.0
    with pytest.raises(b.Unsupported, match="maxDamage"):
        b.convert_variant(m, [0, 0, 0])


def test_filters_and_effects():
    proj_extra = [T("SlowModel", multiplier=0.5, lifespan=11.0, glueLevel=1, layers=3, chance=0.0),
                  T("SlowModifierForTagModel", tag="Moabs", slowMultiplier=1.0, makeNotTag=False),
                  T("DamageModifierForTagModel", tag="Ceramic", damageAddative=3.0, damageMultiplier=1.0)]
    m = dart_like(proj_extra=proj_extra)
    m["behaviors"][0]["weapons"][0]["projectile"]["filters"] += [T("FilterOutTagModel", tag="Moabs"),
                                                                 T("FilterGlueLevelModel", glueLevel=1)]
    ex = b.convert_variant(m, [0, 0, 0])["attacks"][0]["extras"]
    assert ex["filter"] == {"exclude_tags": ["Moabs"], "skip_glue_level": 1}
    assert ex["damage_vs"] == [{"tag": "Ceramic", "add": 3.0}]
    assert ex["slow"]["multiplier_for"] == [{"tag": "Moabs", "factor": 1.0}]


def test_income_thresholds(tmp_path):
    import json
    (tmp_path / "rounds" / "R").mkdir(parents=True)
    (tmp_path / "gamedata").mkdir()
    (tmp_path / "rounds" / "R" / "_set.json").write_text(json.dumps({"linkedIncomeSet": "I"}))
    (tmp_path / "gamedata" / "incomeSets.json").write_text(json.dumps([
        {"name": "I", "thresholds": [{"threshold": 50, "multiplier": 1.0}, {"threshold": 60, "multiplier": 0.5}],
         "finalMultiplier": 0.2}]))
    inc = b.build_income(tmp_path, "R", 61)
    assert [inc[i]["cash_per_pop"] for i in (0, 49, 50, 59, 60)] == [1.0, 1.0, 0.5, 0.5, 0.2]
