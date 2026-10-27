"""Golden-Tests für tools/normalize.py: eine Fixture pro Agent-Konvention plus Invarianten
über die erzeugten Basisdaten.

    .venv/bin/python -m pytest tests
"""
import json
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
import normalize as nz  # noqa: E402


def attack(name="attack", **kw):
    a = {"name": name, "damage": 1, "pierce": 2, "cooldown": 1.0, "damage_type": "sharp",
         "projectiles": 1, "blast_radius": None, "notes": ""}
    a.update(kw)
    return a


def tower(attacks, top=(), middle=(), bottom=(), camo=False, rng=40):
    def path(changes):
        changes = list(changes) + [{}] * (5 - len(changes))
        return [{"tier": i + 1, "name": f"T{i + 1}", "cost": 100, "changes": c} for i, c in enumerate(changes)]
    return {"name": "Test", "category": "primary", "cost": 200, "footprint_radius": 6, "placement": "land",
            "range": rng, "camo": camo, "attacks": attacks, "abilities": [],
            "upgrades": {"top": path(top), "middle": path(middle), "bottom": path(bottom)},
            "source_urls": [], "game_version": "55.1"}


def run(raw):
    log = nz.Log()
    return nz.normalize(raw, "test_tower", log), log


def state(doc, path, tier):
    return doc["paths"][path][tier - 1]["state"]


# ---------------------------------------------------------------- Wertkonventionen
def test_absolute_values_primary_style():
    doc, log = run(tower([attack("dart")], top=[{"pierce": 3}, {"pierce": 5}]))
    assert state(doc, "top", 2)["attacks"][0]["pierce"] == 5
    assert doc["paths"]["top"][1]["ops"] == [{"target": "dart", "stat": "pierce", "op": "set", "value": 5}]
    assert not log.unparsed


def test_delta_strings():
    doc, log = run(tower([attack()], top=[{"pierce": "+1"}, {"cooldown": "x0.5"}, {"range": "+10%"}]))
    s = state(doc, "top", 3)
    assert s["attacks"][0]["pierce"] == 3
    assert s["attacks"][0]["cooldown"] == 0.5
    assert s["range"] == 44
    ops = [o for t in doc["paths"]["top"][:3] for o in t["ops"]]
    assert {"target": "attack", "stat": "cooldown", "op": "mul", "value": 0.5} in ops
    assert {"target": "tower", "stat": "range", "op": "mul", "value": 1.1} in ops
    assert not log.unparsed


def test_arrow_takes_end_value_and_checks_start():
    doc, log = run(tower([attack(cooldown=0.2)], top=[{"cooldown": "x0.66 (0.2 -> 0.132)"}]))
    assert state(doc, "top", 1)["attacks"][0]["cooldown"] == 0.132
    assert not log.unparsed
    _, log = run(tower([attack(cooldown=0.3)], top=[{"cooldown": "x0.66 (0.2 -> 0.132)"}]))
    assert any("Startwert" in u for u in log.unparsed)


def test_dotted_key_targets_named_attack():
    raw = tower([attack("Attack/Projectile"), attack("Attack/Shrapnel", damage=1)],
                top=[{"Attack/Shrapnel.damage": "+2"}])
    doc, _ = run(raw)
    atks = {a["tag"]: a for a in state(doc, "top", 1)["attacks"]}
    assert atks["attack_shrapnel"]["damage"] == 3
    assert atks["attack_projectile"]["damage"] == 1


def test_delta_without_old_value_is_reported():
    _, log = run(tower([attack(pierce=None)], top=[{"pierce": "+1"}]))
    assert any("ohne alten Zahlenwert" in u for u in log.unparsed)


def test_delta_with_trailing_text_is_reported():
    _, log = run(tower([attack()], top=[{"damage": "+5 (vs MOAB only)"}]))
    assert any("Zusatztext" in u for u in log.unparsed)


def test_unknown_dotted_attack_is_reported():
    _, log = run(tower([attack()], top=[{"Ghost.damage": "+1"}]))
    assert any("nicht gefunden" in u for u in log.unparsed)


def test_rounding_residue_is_cleaned():
    doc, _ = run(tower([attack(cooldown=4.9999)], top=[{"cooldown": "x0.5"}]))
    assert state(doc, "top", 1)["attacks"][0]["cooldown"] == 2.5


# ---------------------------------------------------------------- Angriffe hinzufügen/entfernen
def test_new_replace_remove_modify_attacks():
    raw = tower([attack("dart")], top=[
        {"new_attack": attack("ball", pierce=10)},
        {"modified_attacks": {"ball": {"pierce": 20}}},
        {"replace_attack": attack("spear")},
        {"removed_attacks": ["ball"]},
    ])
    doc, log = run(raw)
    assert [a["tag"] for a in state(doc, "top", 1)["attacks"]] == ["dart", "ball"]
    assert state(doc, "top", 2)["attacks"][1]["pierce"] == 20
    assert [a["tag"] for a in state(doc, "top", 3)["attacks"]] == ["spear", "ball"]
    assert [a["tag"] for a in state(doc, "top", 4)["attacks"]] == ["spear"]
    assert not log.unparsed


# ---------------------------------------------------------------- Notizen
PRIMARY_GLUE = ("cannot target Camo; wiki stats: Cooldown: 0.5s; Targeting range: 46 units; "
                "Projectile {Pierce: 2; Speed: 300 units/s; Lifespan: 0.43s; Filters: Cannot hit Camo; "
                "Damage over time {Damage: 1; Damage to Ceramic: +2; Duration: 11s}}")


def test_nested_blocks_do_not_leak_into_main_attack():
    doc, _ = run(tower([attack("glue", damage=0, notes=PRIMARY_GLUE)], rng=46))
    a = doc["base"]["attacks"][0]
    assert a["damage_vs"] == {}
    assert a["projectile_speed"] == 300 and a["lifespan"] == 0.43
    assert a["range"] is None  # gleich der Turm-Range -> kein Override


def test_explosion_block_is_damage_source_for_splash():
    notes = ("Projectile {Pierce: 1; Speed: 180 units/s; Lifespan: 0.6s}; "
             "Explosion {Pierce: 22; Damage: 1; Damage to Ceramic: +3; Radius: 12 units}")
    doc, _ = run(tower([attack("bomb", blast_radius=12, notes=notes)]))
    a = doc["base"]["attacks"][0]
    assert a["damage_vs"] == {"ceramic": 3}
    assert a["projectile_speed"] == 180


@pytest.mark.parametrize("notes, expected", [
    ("+30 Ceramic, +15 Fortified; lifespan 50s", {"ceramic": 30, "fortified": 15}),
    ("+3 damage to Ceramic", {"ceramic": 3}),
    ("+80 MOAB-class damage", {"moab": 80}),
    ("explodes (+7 MOAB)", {"moab": 7}),
    ("+15 damage vs fortified MOAB-class", {"fortified+moab": 15}),
    ("+9 damage vs lead/DDT", {"lead": 9, "ddt": 9}),
    ("+2 damage vs Ceramic or Fortified", {"ceramic": 2, "fortified": 2}),
    ("Damage to Ceramic: +8; Damage to Fortified: +5", {"ceramic": 8, "fortified": 5}),
    ("+1 pierce; +10% attack speed", {}),
])
def test_damage_vs_formats(notes, expected):
    doc, _ = run(tower([attack(notes=notes)]))
    assert doc["base"]["attacks"][0]["damage_vs"] == expected


def test_speed_formats_and_exclusions():
    for notes, speed in [("speed 350; lifespan 1.1429s", 350), ("speed=200; lifespan=0.4s", 200),
                         ("projectile speed 375, lifespan 0.24s", 375), ("attack speed 2", None),
                         ("flight speed 80", None)]:
        doc, _ = run(tower([attack(notes=notes)]))
        assert doc["base"]["attacks"][0]["projectile_speed"] == speed, notes


def test_notes_alternation_does_not_replace_notes():
    doc, _ = run(tower([attack(notes="speed 300")], top=[{"attack.notes_alternation": "every 4th"}]))
    a = state(doc, "top", 1)["attacks"][0]
    assert "speed 300" in a["notes"] and "every 4th" in a["notes"]


# ---------------------------------------------------------------- Camo
def test_camo_upgrade_clears_attack_restriction():
    raw = tower([attack(notes="cannot hit Camo")], middle=[{"camo": True, "attack.notes": "+2 damage vs Camo"}])
    doc, log = run(raw)
    s = state(doc, "middle", 1)
    assert s["camo"] is True
    assert s["attacks"][0]["camo"] is None
    assert s["attacks"][0]["damage_vs"] == {"camo": 2}
    assert not log.camo_exceptions


def test_camo_upgrade_without_notes_update_clears_old_restriction():
    raw = tower([attack(notes="cannot hit Camo")], middle=[{"camo": True}])
    doc, _ = run(raw)
    assert state(doc, "middle", 1)["attacks"][0]["camo"] is None


def test_explicit_contradiction_is_kept_and_reported():
    raw = tower([attack(notes="cannot hit Camo")], bottom=[{"camo": True, "attack.notes": "cannot hit Camo"}])
    doc, log = run(raw)
    assert state(doc, "bottom", 1)["attacks"][0]["camo"] is False
    assert log.camo_exceptions


# ---------------------------------------------------------------- Invarianten über die echten Daten
@pytest.fixture(scope="module")
def generated():
    docs, logs = {}, {}
    for f in sorted(nz.RAW.glob("*.json")):
        logs[f.stem] = nz.Log()
        docs[f.stem] = nz.normalize(json.loads(f.read_text()), f.stem, logs[f.stem])
    return docs, logs


def test_generated_data_matches_schema(generated):
    pytest.importorskip("jsonschema")
    docs, _ = generated
    assert nz.validate(docs) == []


def test_generated_data_has_no_unparsed_values(generated):
    _, logs = generated
    assert [u for l in logs.values() for u in l.unparsed] == []


def test_committed_base_is_up_to_date(generated):
    docs, _ = generated
    for tid, doc in docs.items():
        committed = json.loads((nz.OUT / f"{tid}.json").read_text())
        assert committed == json.loads(json.dumps(doc)), f"{tid}: data/base veraltet, normalize.py neu laufen lassen"


def test_attack_camo_false_only_as_reported_exception(generated):
    docs, logs = generated
    for tid, doc in docs.items():
        for p, tiers in doc["paths"].items():
            for t in tiers:
                s = t["state"]
                for a in s["attacks"]:
                    if a["camo"] is False:
                        assert s["camo"], f"{tid} {p}{t['tier']}: camo=False ohne Turm-Camo ist redundant"
                        assert any(f"{p}{t['tier']}" in e for e in logs[tid].camo_exceptions)


def test_known_values(generated):
    docs, _ = generated
    dart5 = docs["dart_monkey"]["paths"]["top"][4]["state"]["attacks"][0]
    assert (dart5["damage"], dart5["pierce"], dart5["damage_vs"]) == (5, 210, {"lead": 20, "ceramic": 8, "fortified": 5})
    sniper = docs["sniper_monkey"]["paths"]["top"]
    assert [t["state"]["attacks"][0]["damage"] for t in sniper] == [4, 7, 20, 30, 140]
    mine = next(a for a in docs["spike_factory"]["paths"]["top"][4]["state"]["attacks"] if a["tag"] == "super_mine")
    assert mine["damage_vs"] == {"ceramic": 30, "fortified": 15}
