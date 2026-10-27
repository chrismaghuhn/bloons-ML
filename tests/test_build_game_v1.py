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
    proj["id"] = "Projectile"
    assert b.convert_variant(m, [0, 0, 0])["attacks"][0]["delivery"] == {"kind": "area", "radius": 34.0}
    # die Kopplung an die Reichweite steht im Dump am Turm und verweist auf das Projektil
    link = T("LinkProjectileRadiusToTowerRangeModel", projectileModel={"id": "Projectile", "radius": 34.0},
             baseTowerRange=32.0, projectileRadiusOffset=2.0)
    m["behaviors"].append(link)
    assert b.convert_variant(m, [0, 0, 0])["attacks"][0]["delivery"] == \
        {"kind": "area", "radius": 34.0, "radius_offset": 2.0}
    link["projectileModel"]["id"] = "Other"  # gehört zu einem anderen Projektil
    assert "radius_offset" not in b.convert_variant(m, [0, 0, 0])["attacks"][0]["delivery"]
    link["projectileModel"]["id"] = "Projectile"
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


def test_trim_path_keeps_the_span_between_spawn_and_leak():
    pts = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]  # Länge 30
    out = b.trim_path(pts, 5.0, 25.0)
    assert out == [[5.0, 0.0], [10.0, 0.0], [10.0, 10.0], [5.0, 10.0]]
    length = sum(((bx - ax) ** 2 + (by - ay) ** 2) ** 0.5 for (ax, ay), (bx, by) in zip(out, out[1:]))
    assert length == pytest.approx(20.0)
    assert b.trim_path(pts, 2.0, 8.0) == [[2.0, 0.0], [8.0, 0.0]], "Start und Ende im selben Segment"
    with pytest.raises(SystemExit):
        b.trim_path(pts, 5.0, 40.0)
    with pytest.raises(SystemExit):
        b.trim_path(pts, 20.0, 10.0)  # vertauscht


def test_bloon_speed_factor_from_match_rules(tmp_path):
    (tmp_path / "matches").mkdir()
    medium = {"mods": [{"$type": "X.GameModModel, A", "mutatorMods": [
        {"$type": "Il2CppAssets.Scripts.Models.Gameplay.Mods.GlobalSpeedModModel, Assembly-CSharp",
         "multiplier": 1.1}]}]}
    (tmp_path / "matches" / "Standard_Standard_Medium_Tutorial.json").write_text(__import__("json").dumps(medium))
    (tmp_path / "matches" / "Standard_Standard_Easy_Tutorial.json").write_text('{"mods": []}')
    assert b.bloon_speed_factor(tmp_path, "Standard_Standard_Medium", "Tutorial") == 1.1
    assert b.bloon_speed_factor(tmp_path, "Standard_Standard_Easy", "Tutorial") == 1.0
    with pytest.raises(SystemExit):
        b.bloon_speed_factor(tmp_path, "Standard_Clicks_Hard", "Tutorial")


def test_presets_name_mode_and_difficulty():
    # meta.mode/difficulty werden aus dem Partie-Namen gelesen: "<Spieltyp>_<Modus>_<Schwierigkeit>"
    for name, p in b.PRESETS.items():
        parts = p["match"].split("_")
        assert len(parts) == 3 and all(parts), name
    assert b.PRESETS["chimps"]["match"].split("_")[1:] == ["Clicks", "Hard"]


# ---------------------------------------------------------------- Türme Schritt 2 (Village, Farm)

def support(typ, **kw):
    base = dict(isUnique=True, isGlobal=False, isCustomRadius=False, appliesToOwningTower=False,
                onlyAffectParagon=False, maxStackSize=0, filters=None)
    return T(typ, **{**base, **kw})


def village_like(*supports_):
    """Support-Turm wie die Village: ein AttackModel ohne Waffen (nur Reichweitenkreis) + Auren."""
    empty = T("AttackModel", range=40.0, weapons=[],
              behaviors=[T("AttackFilterModel", filters=[T("FilterInvisibleModel", isActive=True)])])
    return T("TowerModel", cost=1200.0, range=40.0, towerSet=8, footprint=T("CircleFootprintModel", radius=11.0),
             behaviors=[empty, *supports_])


PRIMARY = [T("FilterInSetModel", towerSets=[1], towerSet=1)]


def test_village_auras_and_discount():
    v = b.convert_variant(village_like(
        support("RangeSupportModel", multiplier=0.1, additive=0.0, mutatorId="Range:Support", maxStacks=0),
        support("RateSupportModel", multiplier=0.85, mutatorId="Village:Rate"),
        support("VisibilitySupportModel", mutatorId="Village:Visibility"),
        support("PierceSupportModel", pierce=1.0, mutatorId="P", filters=PRIMARY),
        support("ProjectileSpeedSupportModel", multiplier=0.25, mutatorId="S", filters=PRIMARY),
        support("DamageTypeSupportModel", immuneBloonProperties=0, mutatorId="Village:DamageType"),
        support("SupportRemoveFilterOutTagModel", removeScriptWithSupportMutatorId="MibMutator", isUnique=None),
        T("DiscountZoneModel", discountMultiplier=0.1, stackLimit=1, stackName="Major", affectSelf=False,
          tierCap=3, tierMin=0, towerBaseIds="", towerBaseIdList=[], isGlobal=False, isGlobalRange=False),
        T("AddBehaviorToBloonInZoneModel", behaviors=[T("GrowBlockModel")]),
    ), [0, 0, 0])
    assert v["attacks"] == [] and v["footprint"] == 11.0
    assert v["supports"] == [
        {"kind": "range", "group": "Range:Support", "add_frac": 0.1, "add": 0.0},
        {"kind": "rate", "group": "Village:Rate", "factor": 0.85},
        {"kind": "camo", "group": "Village:Visibility"},
        {"kind": "pierce", "group": "P", "sets": ["primary"], "add": 1},
        {"kind": "projectile_speed", "group": "S", "sets": ["primary"], "add_frac": 0.25},
        {"kind": "damage_all", "group": "Village:DamageType"},
    ]
    assert v["discount"] == [{"fraction": 0.1, "tier_min": 0, "tier_cap": 3, "stack": "Major", "stack_limit": 1,
                              "affect_self": False}]


@pytest.mark.parametrize("model, why", [
    (support("RateSupportModel", multiplier=0.85, mutatorId="R", isGlobal=True), "global"),
    (support("RateSupportModel", multiplier=0.85, mutatorId="R", isUnique=False), "isUnique"),
    (support("RateSupportModel", multiplier=1.2, mutatorId="R"), "multiplier"),
    (support("VisibilitySupportModel", mutatorId="V", filters=[T("FilterInvisibleModel")]), "Support-Filter"),
    (support("DamageTypeSupportModel", immuneBloonProperties=1, mutatorId="D"), "Immunitäten"),
    (T("DiscountZoneModel", discountMultiplier=0.1, stackLimit=1, stackName="M", tierCap=3, tierMin=0,
       towerBaseIdList=["DartMonkey"]), "bestimmte Türme"),
    (T("AddBehaviorToBloonInZoneModel", behaviors=[T("SlowModel")]), "GrowBlock"),
])
def test_unsupported_auras_are_refused(model, why):
    with pytest.raises(b.Unsupported, match=why):
        b.convert_variant(village_like(model), [0, 0, 0])


def test_tower_without_attack_support_or_income_is_refused():
    with pytest.raises(b.Unsupported, match="kein Angriff"):
        b.convert_variant(village_like(), [0, 0, 0])


def test_support_models_are_strict_elsewhere():
    """Ein Auren-Modell unterhalb eines Projektils (nicht am Turm) bleibt gesperrt."""
    hidden = support("RateSupportModel", multiplier=0.85, mutatorId="R")
    with pytest.raises(b.Unsupported, match="RateSupportModel"):
        b.convert_variant(dart_like(proj_extra=[hidden]), [0, 0, 0])


def test_filter_disabled_by_mib_is_refused():
    out = T("FilterOutTagModel", tag="Ddt", disableWhenSupportMutatorIDs=["MibMutator"])
    with pytest.raises(b.Unsupported, match="MIB"):
        b.convert_filters([out])
    out["disableWhenSupportMutatorIDs"] = ["false"]  # Ice: anderer, wirkungsloser Eintrag
    assert b.convert_filters([out]) == (True, {"exclude_tags": ["Ddt"]})


def farm_like(count=4, value=20.0, bonus=0.0, extra=()):
    """Farm: Bananen-Waffe (CashModel je Banane, EmissionsPerRoundFilter) und Rechteck-Footprint."""
    proj = T("ProjectileModel", pierce=9999999.0, radius=5.0, behaviors=[
        T("PickupModel", collectRadius=50.0), T("CashModel", minimum=value, maximum=value, bonusMultiplier=bonus),
        T("AgeModel", lifespan=15.0), T("ScaleProjectileModel"), T("ArriveAtTargetModel"),
        T("HeightOffsetProjectileModel")])
    weapon = T("WeaponModel", rate=0.05, emission=T("SingleEmmisionTowardsTargetModel"), projectile=proj,
               behaviors=[T("EmissionsPerRoundFilterModel", count=count), T("WeaponRateMinModel", min=0.05)])
    pos = T("RandomPositionBasicModel", minRadius=20.0, maxRadius=40.0)
    attack = T("AttackModel", range=40.0, weapons=[weapon], targetProvider=pos, behaviors=[pos])
    rect = T("RectangleFootprintModel", xWidth=30.0, yWidth=30.0)
    return T("TowerModel", cost=1250.0, range=40.0, towerSet=8, footprint=rect,
             behaviors=[rect, attack, *extra])


def test_farm_income_and_footprint():
    v = b.convert_variant(farm_like(count=4, value=20.0, bonus=0.25), [0, 2, 0])
    assert v["attacks"] == [] and v["income"] == {"per_round": 100.0}
    assert v["footprint"] == pytest.approx(15.0 * 2 ** 0.5) and v["_rect"] is True
    bonus = [T("PerRoundCashBonusTowerModel", cashPerRound=500.0), T("BonusLivesPerRoundModel", amount=15),
             T("CollectCashZoneModel")]
    v = b.convert_variant(farm_like(count=16, value=70.0, extra=bonus), [0, 0, 5])
    assert v["income"] == {"per_round": 16 * 70.0 + 500.0, "lives": 15}
    m = farm_like()
    m["sellbackModifierAdd"] = 0.1
    assert b.convert_variant(m, [0, 0, 2])["sell_bonus"] == 0.1


def test_farm_income_edge_cases_are_refused():
    m = farm_like()
    m["behaviors"][1]["weapons"][0]["projectile"]["behaviors"][1]["maximum"] = 30.0
    with pytest.raises(b.Unsupported, match="zufälligem"):
        b.convert_variant(m, [0, 0, 0])
    with pytest.raises(b.Unsupported, match="ohne Bananen"):
        b.convert_variant(village_like(support("VisibilitySupportModel", mutatorId="V"),
                                       T("PerRoundCashBonusTowerModel", cashPerRound=1.0)), [0, 0, 0])
    # Bananen-Modelle sind nur in der Einkommens-Waffe erlaubt
    with pytest.raises(b.Unsupported, match="PickupModel"):
        b.convert_variant(dart_like(proj_extra=[T("PickupModel")]), [0, 0, 0])


def write_match(tmp_path, name, mutators, fields=None):
    import json
    (tmp_path / "matches").mkdir(exist_ok=True)
    mods = [{"$type": "X.ModModel, A", "mutatorMods": [
        {"$type": f"Il2CppAssets.Scripts.Models.Gameplay.Mods.{t}, Assembly-CSharp", **kw} for t, kw in mutators]}]
    (tmp_path / "matches" / f"{name}_Tutorial.json").write_text(json.dumps({"mods": mods, "fields": fields or {}}))


def test_match_rules_selling_and_locks(tmp_path):
    write_match(tmp_path, "Standard_Clicks_Hard", [("LockTowerModModel", {"towerToLock": "BananaFarm"}),
                                                   ("DisableSellTowerModModel", {}),
                                                   ("SellMultiplierModModel", {"multiplier": 0.0, "addition": 0.0})],
                {"towerSellEnabled": False})
    assert b.match_rules(tmp_path, "Standard_Clicks_Hard", "Tutorial") == (False, ["BananaFarm"])
    write_match(tmp_path, "Standard_Standard_Medium", [("SellMultiplierModModel", {"addition": 0.7})],
                {"towerSellEnabled": True})
    assert b.match_rules(tmp_path, "Standard_Standard_Medium", "Tutorial") == (True, [])
    write_match(tmp_path, "Bad", [("DisableSellTowerModModel", {})], {"towerSellEnabled": True})
    with pytest.raises(SystemExit, match="towerSellEnabled"):
        b.match_rules(tmp_path, "Bad", "Tutorial")
    write_match(tmp_path, "Only", [("LockTowerSetModModel", {"towerSet": 4})])
    with pytest.raises(SystemExit, match="LockTowerSetModModel"):
        b.match_rules(tmp_path, "Only", "Tutorial")


def test_income_weapon_must_be_alone_and_plain():
    """Bananen-Modelle werden nur in der Einkommens-Waffe freigegeben, nicht in Nachbar-Waffen."""
    m = farm_like()
    dart_weapon = dart_like()["behaviors"][0]["weapons"][0]
    m["behaviors"][1]["weapons"].append(dart_weapon)
    with pytest.raises(b.Unsupported, match="anderen Waffen"):
        b.convert_variant(m, [0, 0, 0])
    m = farm_like(extra=[T("PerRoundCashBonusTowerModel", cashPerRound=1.0, distributeCash=True)])
    with pytest.raises(b.Unsupported, match="distributeCash"):
        b.convert_variant(m, [0, 0, 0])
    m = farm_like()
    m["footprint"]["ignoresTowerOverlap"] = True
    with pytest.raises(b.Unsupported, match="Platzierung"):
        b.convert_variant(m, [0, 0, 0])
    m = farm_like()
    m["behaviors"][1]["weapons"][0]["projectile"]["behaviors"][1]["emittedByCashEarnedMultiplier"] = 2.0
    with pytest.raises(b.Unsupported, match="emittedByCashEarnedMultiplier"):
        b.convert_variant(m, [0, 0, 0])


def test_remove_filter_support_only_for_mib():
    other = support("SupportRemoveFilterOutTagModel", removeScriptWithSupportMutatorId="HitBlack", isUnique=None)
    with pytest.raises(b.Unsupported, match="anderer Mutator"):
        b.convert_variant(village_like(support("VisibilitySupportModel", mutatorId="V"), other), [0, 0, 0])


def test_two_radius_links_for_one_projectile_are_refused():
    m = dart_like()
    proj = proj_of(m)
    proj["behaviors"] = [x for x in proj["behaviors"] if "TravelStrait" not in x["$type"]]
    proj.update(radius=34.0, id="Projectile")
    link = dict(projectileModel={"id": "Projectile", "radius": 34.0}, baseTowerRange=32.0, projectileRadiusOffset=2.0)
    m["behaviors"] += [T("LinkProjectileRadiusToTowerRangeModel", **link)] * 2
    with pytest.raises(b.Unsupported, match="mehrere LinkProjectileRadius"):
        b.convert_variant(m, [0, 0, 0])


def test_match_rules_checks_sell_ratio(tmp_path):
    write_match(tmp_path, "Odd", [], {"towerSellEnabled": True, "sellMultiplier": 0.5})
    with pytest.raises(SystemExit, match="sellMultiplier"):
        b.match_rules(tmp_path, "Odd", "Tutorial")


def test_placeholder_for_tower_without_mechanic():
    # Heli-ähnlich: unbekannte Modelle (Flieger), Rechteck-Footprint -> Platzhalter ohne Wirkung mit echtem
    # Footprint (Außenkreis) und echter Reichweite (docs/design-towers-step3.md §4)
    rect = T("RectangleFootprintModel", xWidth=27.0, yWidth=27.0)
    heli = T("TowerModel", cost=1500.0, range=22.0, towerSet=2, footprint=rect,
             behaviors=[rect, T("AirUnitModel", behaviors=[T("HeliMovementModel", maxSpeed=40.0)])])
    with pytest.raises(b.Unsupported, match="AirUnitModel"):
        b.convert_variant(heli, [0, 0, 0])
    v = b.placeholder(heli)
    assert v == {"tiers": [0, 0, 0], "range": 22.0, "footprint": pytest.approx(27.0 * 2 ** 0.5 / 2),
                 "attacks": [], "_rect": True}
    assert b.placeholder(dart_like())["footprint"] == 6.0
    # alle t21-Türme ohne Mechanik sind fest gesperrt, Engineer (Basis geht) nicht
    assert set(b.T21) - set(b.T14) - b.MECHANIC_LOCKED == {"EngineerMonkey"}
