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
                  T("SlowModifierForTagModel", tag="Moabs", slowMultiplier=1.0, makeNotTag=False,
                    resetToUnmodified=True),
                  T("DamageModifierForTagModel", tag="Ceramic", damageAddative=3.0, damageMultiplier=1.0)]
    m = dart_like(proj_extra=proj_extra)
    m["behaviors"][0]["weapons"][0]["projectile"]["filters"] += [T("FilterOutTagModel", tag="Moabs"),
                                                                 T("FilterGlueLevelModel", glueLevel=1)]
    ex = b.convert_variant(m, [0, 0, 0])["attacks"][0]["extras"]
    assert ex["filter"] == {"exclude_tags": ["Moabs"], "skip_glue_level": 1}
    assert ex["damage_vs"] == [{"tag": "Ceramic", "add": 3.0}]
    assert ex["slow"]["multiplier_for"] == [{"tag": "Moabs", "factor": 1.0}]


def proj_of(m):
    return m["behaviors"][0]["weapons"][0]["projectile"]


def test_max_pierce_caps_pierce():
    m = dart_like()
    proj_of(m).update(pierce=4.0, maxPierce=1.0)
    assert b.convert_variant(m, [0, 0, 0])["attacks"][0]["pierce"] == 1


def test_area_radius_comes_from_projectile():
    m = dart_like()
    proj = proj_of(m)
    proj["behaviors"] = [x for x in proj["behaviors"] if "TravelStrait" not in x["$type"]]
    proj["radius"] = 34.0
    proj["behaviors"].append(T("LinkProjectileRadiusToTowerRangeModel", projectileRadiusOffset=2.0))
    assert b.convert_variant(m, [0, 0, 0])["attacks"][0]["delivery"] == {"kind": "area", "radius": 34.0}
    proj["radius"] = 32.0  # passt nicht zu Reichweite 32 + Versatz 2
    with pytest.raises(b.Unsupported, match="LinkProjectileRadius"):
        b.convert_variant(m, [0, 0, 0])


def test_damage_type_filter_uses_its_own_mask():
    m = dart_like()
    proj_of(m)["filters"].append(T("FilterBloonIfDamageTypeModel", ifCantHitBloonProperties=1))
    assert b.convert_variant(m, [0, 0, 0])["attacks"][0]["extras"]["filter"]["skip_props"] == ["lead"]


def test_slow_modifiers_reset_scale_and_override():
    slow = T("SlowModel", multiplier=0.25, lifespan=18.0, glueLevel=5, layers=3, chance=0.0, mutationId="Glue")
    mods = [T("SlowModifierForTagModel", tag="Moabs", slowId="Glue", slowMultiplier=2.5, lifespanOverride=6.75),
            T("SlowModifierForTagModel", tag="Bad", slowId="Glue", slowMultiplier=1.6, resetToUnmodified=True),
            T("SlowModifierForTagModel", tag="Boss", slowId="Glue", slowMultiplier=1.0, preventMutation=True)]
    sl = b.convert_variant(dart_like(proj_extra=[slow, *mods]), [0, 0, 0])["attacks"][0]["extras"]["slow"]
    assert sl["multiplier_for"] == [{"tag": "Moabs", "factor": 0.625}, {"tag": "Bad", "factor": 1.0},
                                    {"tag": "Boss", "factor": 1.0}]
    assert sl["seconds_for"] == [{"tag": "Moabs", "factor": 6.75}] and sl["id"] == "Glue"
    other = T("SlowModifierForTagModel", tag="Moabs", slowId="Stun", slowMultiplier=1.0)
    with pytest.raises(b.Unsupported, match="fremder Slow"):
        b.convert_variant(dart_like(proj_extra=[slow, other]), [0, 0, 0])
    with pytest.raises(b.Unsupported, match="mehrere SlowModel"):
        b.convert_variant(dart_like(proj_extra=[slow, slow]), [0, 0, 0])


def corrosive(**kw):
    dot = T("DamageOverTimeModel", **{"damage": 1.0, "interval": 2.0, "immuneBloonProperties": 64, "payloadCount": 0,
                                      "triggerImmediate": False, "initialDelay": 0.0, "distributeToChildren": True,
                                      "damageModifierModels": None, **kw})
    return T("AddBehaviorToBloonModel", mutationId="CorrosiveDot", layers=9999999, lifespan=11.0, glueLevel=2,
             chance=0.0, behaviors=[dot])


def test_damage_over_time_and_its_tag_modifier():
    mod = T("AddBehaviorModifierForTagModel", tag="Moabs", mutatorId="CorrosiveDot", mutatorMultiplier=0.375)
    dot = b.convert_variant(dart_like(proj_extra=[corrosive(), dict(corrosive(), name="x"), mod]),
                            [0, 0, 0])["attacks"][0]["extras"]["dot"]
    assert dot == {"damage": 1.0, "interval": 2.0, "seconds": 11.0, "layers": 9999999, "level": 2,
                   "factor_for": [{"tag": "Moabs", "factor": 0.375}]}
    # Modifier ohne passenden Effekt bleibt wirkungslos
    assert "dot" not in b.convert_variant(dart_like(proj_extra=[mod]), [0, 0, 0])["attacks"][0]["extras"]
    with pytest.raises(b.Unsupported, match="DamageOverTimeModel"):
        b.convert_variant(dart_like(proj_extra=[corrosive(triggerImmediate=True)]), [0, 0, 0])


def test_push_back_with_moab_multipliers():
    push = T("PushBackModel", pushAmount=20.0, tag="", multiplierMOAB=0.0, multiplierBFB=0.0, multiplierDDT=1.0,
             multiplierZOMG=0.0, onlyIfDamaged=True)
    p = b.convert_variant(dart_like(proj_extra=[push]), [0, 0, 0])["attacks"][0]["extras"]["push"]
    assert p == {"amount": 20.0, "factor_for": [{"tag": "Moab", "factor": 0.0}, {"tag": "Bfb", "factor": 0.0},
                                                {"tag": "Zomg", "factor": 0.0}]}
    with pytest.raises(b.Unsupported, match="PushBackModel"):
        b.convert_variant(dart_like(proj_extra=[dict(push, onlyIfDamaged=False)]), [0, 0, 0])


def test_crit_parallel_and_clear_hits():
    m = dart_like(emission=T("ParallelEmissionModel", count=2, spreadLength=3.0, offsetStart=-1.5, linear=False),
                  proj_extra=[T("ClearHitBloonsModel", interval=0.3)])
    m["behaviors"][0]["weapons"][0]["behaviors"] = [T("CritMultiplierModel", damage=80.0, lower=4, upper=6)]
    a = b.convert_variant(m, [0, 0, 0])["attacks"][0]
    assert a["crit"] == {"damage": 80.0, "every": 5, "every_range": [4, 6]}
    assert a["delivery"]["emission"] == {"kind": "parallel", "count": 2, "spread": 3.0}
    assert a["delivery"]["clear_hits"] == 0.3


def test_cluster_shards_explode_when_exhausted():
    explosion = T("ProjectileModel", pierce=8.0, radius=15.0, filters=[],
                  behaviors=[T("DamageModel", damage=1.0, maxDamage=0.0, immuneBloonProperties=2),
                             T("AgeModel", lifespan=0.1)])
    cluster = T("ProjectileModel", pierce=1.0, maxPierce=1.0, radius=5.0, dontUseCollisionChecker=True, behaviors=[
        T("CreateProjectileOnExhaustFractionModel", projectile=explosion, emission=T("SingleEmissionModel"),
          fraction=1.0, durationfraction=-1.0),
        T("RandomRangeTravelStraitModel", minRange=22.0, maxRange=40.0, speed=130.0)])
    contact = T("CreateProjectileOnContactModel", projectile=cluster, passOnCollidedWith=True,
                emission=T("ArcEmissionModel", count=8, angle=360.0, offset=0.0, useProjectileRotation=False))
    c = b.convert_variant(dart_like(proj_extra=[contact]), [0, 0, 0])["attacks"][0]["delivery"]["on_contact"][0]
    assert (c["kind"], c["speed"], c["lifespan"]) == ("shards", 130.0, 31.0 / 130.0)
    assert [e["kind"] for e in c["on_exhaust"]] == ["explosion"] and c["on_exhaust"][0]["pierce"] == 8
    assert c["collide"] is False
    # ohne Verbrauchs-Effekt wäre ein Projektil ohne Kollision wirkungslos -> gesperrt
    cluster["behaviors"] = cluster["behaviors"][1:]
    with pytest.raises(b.Unsupported, match="ohne Kollision"):
        b.convert_variant(dart_like(proj_extra=[contact]), [0, 0, 0])


def test_slow_zone_bound_to_tower_range():
    zone = T("SlowBloonsZoneModel", zoneRadius=25.0, speedScale=0.6, speedChange=0.0, bindRadiusToTowerRange=True,
             radiusOffset=5.0, bloonTag="", bloonTags=[],
             filters=[T("FilterInvisibleModel", isActive=True), T("FilterOutTagModel", tag="Lead")])
    v = b.convert_variant(dart_like(extra_behaviors=[zone]), [0, 0, 0])
    assert v["zones"] == [{"radius": 37.0, "speed_scale": 0.6, "camo": False, "filter": {"exclude_tags": ["Lead"]}}]


def test_ease_path_is_refused():
    fp = T("FollowPathModel", speed=100.0, path=[{"x": 0.0, "y": 0.0}, {"x": 0.0, "y": 50.0}],
           easePath=[{"x": 1.0, "y": 0.0}], destroyAtEndOfPath=True)
    m = dart_like()
    proj = proj_of(m)
    proj["behaviors"] = [x for x in proj["behaviors"] if "TravelStrait" not in x["$type"]] + [fp]
    with pytest.raises(b.Unsupported, match="easePath"):
        b.convert_variant(m, [0, 0, 0])


def test_prune_tags_drops_unknown_tags():
    towers = [{"extras": {"filter": {"exclude_tags": ["Moabs", "Boss"]}, "damage_vs": [{"tag": "Boss", "add": 1}],
                          "slow": {"multiplier_for": [{"tag": "Moabs", "factor": 1.0}]}}}]
    dropped = set()
    b.prune_tags(towers, {"Moabs"}, dropped)
    assert dropped == {"Boss"}
    assert towers[0]["extras"]["filter"]["exclude_tags"] == ["Moabs"] and towers[0]["extras"]["damage_vs"] == []
    # "Moabs" (Klasse) kommt hinter speziellere Tags, die Engine nimmt den ersten Treffer
    slow = {"multiplier_for": [{"tag": "Moabs", "factor": 0.625}, {"tag": "Bad", "factor": 1.0}]}
    b.prune_tags(slow, {"Moabs", "Bad"}, set())
    assert [e["tag"] for e in slow["multiplier_for"]] == ["Bad", "Moabs"]


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


def sniper_like(burst=None, extra_filter=True, contact=False):
    """Sofort-Treffer wie Sniper/Desperado: InstantModel + FilterAllExceptTarget, InstantDamageEmission."""
    only = T("FilterAllExceptTargetModel")
    proj = T("ProjectileModel", pierce=5.0, radius=0.0,
             filters=[T("FilterInvisibleModel", isActive=True)] + ([only] if extra_filter else []),
             behaviors=[T("DamageModel", damage=2.0, maxDamage=0.0, immuneBloonProperties=0),
                        T("InstantModel", destroyIfInvalid=False),
                        T("ProjectileFilterModel", filters=[T("FilterAllExceptTargetModel")])])
    if contact:
        proj["behaviors"].append(T("CreateProjectileOnContactModel", projectile=T("ProjectileModel", pierce=1.0)))
    weapon = T("WeaponModel", rate=1.59, emission=T("InstantDamageEmissionModel"), projectile=proj,
               behaviors=[burst] if burst else [])
    attack = T("AttackModel", range=9999999.0, name="AttackModel_Attack", weapons=[weapon],
               behaviors=[T("RotateToTargetModel"), T("CheckTargetsWithoutOffsetsModel")])
    return T("TowerModel", cost=350.0, range=20.0, footprint=T("CircleFootprintModel", radius=6.0),
             behaviors=[attack, T("TargetDesperadoFirstModel"), T("TowerRadiusModel")])


def test_instant_hit_targets_only_the_target():
    a = b.convert_variant(sniper_like(), [0, 0, 0])["attacks"][0]
    assert a["delivery"] == {"kind": "hitscan"}
    assert (a["pierce"], a["range"], a["damage"], a["camo"]) == (1, 9999999.0, 2.0, False)
    m = sniper_like(extra_filter=False)
    proj_of(m)["behaviors"] = [x for x in proj_of(m)["behaviors"] if "ProjectileFilter" not in x["$type"]]
    with pytest.raises(b.Unsupported, match="ohne FilterAllExceptTarget"):
        b.convert_variant(m, [0, 0, 0])
    with pytest.raises(b.Unsupported, match="Kontakt"):
        b.convert_variant(sniper_like(contact=True), [0, 0, 0])


def test_burst_and_offset_on_the_weapon():
    burst = T("BurstWeaponBehaviorModel", interval=0.1, count=2, alwaysEmptyClip=False)
    assert b.convert_variant(sniper_like(burst=burst), [0, 0, 0])["attacks"][0]["burst"] == {"count": 2, "interval": 0.1}
    with pytest.raises(b.Unsupported, match="alwaysEmptyClip"):
        b.convert_variant(sniper_like(burst=dict(burst, alwaysEmptyClip=True)), [0, 0, 0])
    m = dart_like()
    m["behaviors"][0]["weapons"][0]["behaviors"] = [T("OffsetModel", range=45.0, angleOffset=0.0)]
    assert "burst" not in b.convert_variant(m, [0, 0, 0])["attacks"][0]
    m["behaviors"][0]["weapons"][0]["behaviors"] = [T("OffsetModel", range=45.0, angleOffset=10.0)]
    with pytest.raises(b.Unsupported, match="angleOffset"):
        b.convert_variant(m, [0, 0, 0])


def test_random_emissions_use_their_mean():
    single = T("RandomArcEmissionModel", randomAngle=8.0, angle=0.0, count=1, offset=0.0, offsetStart=0.0,
               sliceSize=0.0, useProjectileRotation=False, useAirUnitRotation=False)
    assert b.convert_variant(dart_like(emission=single), [0, 0, 0])["attacks"][0]["delivery"]["emission"] == {
        "kind": "single"}
    thorns = T("RandomEmissionModel", angle=25.0, count=5, useSpeedMultiplier=False, useMainAttackRotation=False)
    em = b.convert_variant(dart_like(emission=thorns), [0, 0, 0])["attacks"][0]["delivery"]["emission"]
    assert (em["kind"], em["count"], em["angle"]) == ("arc", 5, 25.0)


def test_strict_models_only_where_they_are_evaluated():
    # Druid-Basis: RemoveBloonModifiers ohne Schalter ist wirkungslos, mit Schalter gesperrt
    rm = T("RemoveBloonModifiersModel", cleanseRegen=False, cleanseCamo=False, cleanseLead=False,
           cleanseFortified=False)
    b.convert_variant(dart_like(proj_extra=[rm]), [0, 0, 0])
    with pytest.raises(b.Unsupported, match="entfernt"):
        b.convert_variant(dart_like(proj_extra=[dict(rm, cleanseCamo=True)]), [0, 0, 0])
    # ein Sofort-Treffer-Modell an anderer Stelle (am Turm statt im Projektil) ist nicht unterstützt
    with pytest.raises(b.Unsupported, match="InstantModel"):
        b.convert_variant(dart_like(extra_behaviors=[T("InstantModel")]), [0, 0, 0])
    # FilterAllExceptTarget an einem fliegenden Projektil wird nicht ausgewertet
    m = dart_like()
    proj_of(m)["filters"].append(T("FilterAllExceptTargetModel"))
    with pytest.raises(b.Unsupported, match="FilterAllExceptTargetModel"):
        b.convert_variant(m, [0, 0, 0])


def test_homing_from_track_target_models():
    wiz = T("TrackTargetModel", distance=80.0, trackNewTargets=True, constantlyAquireNewTarget=False,
            maxSeekAngle=360.0, ignoreSeekAngle=False, overrideRotation=False, useLifetimeAsDistance=False,
            expireOnContactTarget=False, turnRate=360.0, turnRatePerFrame=0.0, TurnRate=360.0)
    h = b.convert_variant(dart_like(proj_extra=[wiz]), [0, 0, 0])["attacks"][0]["delivery"]["homing"]
    assert h == {"turn_rate": 360.0, "distance": 80.0, "max_angle": 360.0, "retarget": True}
    ninja = T("TrackTargetWithinTimeModel", timeInFrames=3.0, distance=9999999.0, trackNewTargets=True,
              constantlyAquireNewTarget=False, maxSeekAngle=144.0, ignoreSeekAngle=False, overrideRotation=False,
              useLifetimeAsDistance=True, expireOnContactTarget=False, turnRatePerFrame=0.0, TurnRate=9999999.0)
    h = b.convert_variant(dart_like(proj_extra=[ninja]), [0, 0, 0])["attacks"][0]["delivery"]["homing"]
    # Reichweite = Flugweite (300 × 0,25), Drehung bis zur Kegelkante (72°) in 3 Frames
    assert (h["distance"], h["max_angle"], h["turn_rate"]) == (75.0, 144.0, 72.0 / 0.05)
    with pytest.raises(b.Unsupported, match="Sonderfälle"):
        b.convert_variant(dart_like(proj_extra=[dict(wiz, constantlyAquireNewTarget=True)]), [0, 0, 0])


def test_homing_inside_shards_is_refused():
    wiz = T("TrackTargetModel", distance=80.0, trackNewTargets=True, maxSeekAngle=360.0, TurnRate=360.0)
    shard = T("ProjectileModel", pierce=1.0, radius=2.0, filters=[],
              behaviors=[T("DamageModel", damage=1.0, maxDamage=0.0, immuneBloonProperties=0),
                         T("TravelStraitModel", speed=300.0, lifespan=0.2), wiz])
    contact = T("CreateProjectileOnContactModel", projectile=shard, passOnCollidedWith=False,
                emission=T("ArcEmissionModel", count=4, angle=360.0, offset=0.0, useProjectileRotation=False))
    with pytest.raises(b.Unsupported, match="TrackTargetModel"):
        b.convert_variant(dart_like(proj_extra=[contact]), [0, 0, 0])
