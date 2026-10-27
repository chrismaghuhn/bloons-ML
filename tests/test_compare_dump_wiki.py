"""mod/tools/compare_dump_wiki.py mit synthetischen Mini-Daten (keine Ninja-Kiwi-Daten im Repo)."""
import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("compare_dump_wiki", ROOT / "mod" / "tools" / "compare_dump_wiki.py")
cdw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cdw)

T = "Il2CppAssets.Scripts.Models.Towers.{}, Assembly-CSharp"


def projectile(damage=1.0, pierce=2.0, children=()):
    behaviors = [{"$type": T.format("Projectiles.Behaviors.DamageModel"), "damage": damage}] if damage is not None else []
    behaviors += [{"$type": T.format("Behaviors.CreateProjectileOnContactModel"), "projectile": c} for c in children]
    return {"$type": T.format("Projectiles.ProjectileModel"), "id": "Projectile", "pierce": pierce, "behaviors": behaviors}


def attack(proj, rate=0.95, camo_filter=True, name="AttackModel_Attack_"):
    filters = [{"$type": T.format("Filters.FilterInvisibleModel"), "isActive": True}] if camo_filter else []
    return {
        "$type": T.format("Behaviors.Attack.AttackModel"),
        "name": name,
        "range": 32.0,
        "weapons": [{"$type": T.format("Weapons.WeaponModel"), "rate": rate, "projectile": proj}],
        "behaviors": [{"$type": T.format("Behaviors.Attack.Behaviors.AttackFilterModel"), "filters": filters}],
    }


def tower(*attacks, cost=200.0, applied=()):
    return {"cost": cost, "range": 32.0, "appliedUpgrades": list(applied), "behaviors": list(attacks)}


def test_match_attack_exact_and_partial():
    entries = cdw.dump_entries(tower(attack(projectile(damage=1, pierce=2), rate=0.95)))
    assert cdw.match_attack({"damage": 1, "pierce": 2, "cooldown": 0.95}, entries)[0] == "match"
    assert cdw.match_attack({"damage": 1, "pierce": 3, "cooldown": 0.95}, entries)[0] == "2/3"


def test_match_attack_without_or_zero_damage_is_weak():
    entries = cdw.dump_entries(tower(attack(projectile(damage=1, pierce=2))))
    assert cdw.match_attack({"pierce": 2, "cooldown": 0.95}, entries)[0] == "weak"
    assert cdw.match_attack({"damage": 0, "pierce": 9, "cooldown": 0.95}, entries)[0] == "weak-mismatch"
    assert cdw.match_attack({"damage": 1}, [])[0] == "no-dump-attack"
    assert cdw.match_attack({"notes": "x"}, entries)[0] == "skip"


def test_sub_projectiles_are_found():
    explosion = projectile(damage=1, pierce=22)
    entries = cdw.dump_entries(tower(attack(projectile(damage=None, pierce=1, children=[explosion]), rate=1.5)))
    assert [(e["damage"], e["pierce"], e["rate"]) for e in entries] == [(1, 22, 1.5)]


def test_dump_camo_uses_first_damaging_attack():
    buff = attack(projectile(damage=None), camo_filter=False, name="Buff")
    assert cdw.dump_camo(tower(buff, attack(projectile(), camo_filter=True))) is False
    assert cdw.dump_camo(tower(attack(projectile(), camo_filter=False))) is True
    assert cdw.dump_camo(tower(buff)) is None


def test_latest_dump_ignores_staging_dirs(tmp_path):
    for name in ("56.1", "57.4", "57.10", "57.10.tmp-20260101T000000Z", "57.11.old-20260101T000000Z"):
        (tmp_path / name).mkdir()
    assert cdw.latest_dump(tmp_path).name == "57.10"  # numerisch, nicht alphabetisch; .tmp/.old ignoriert
    (tmp_path / "57.4b").mkdir()  # gemischte Namen dürfen die Sortierung nicht sprengen
    assert cdw.latest_dump(tmp_path).name == "57.10"
    with pytest.raises(SystemExit):
        cdw.latest_dump(tmp_path / "missing")


def test_compare_end_to_end(tmp_path):
    dump = tmp_path / "dump"
    (dump / "towers" / "DartMonkey").mkdir(parents=True)
    (dump / "upgrades").mkdir()
    (dump / "towers" / "DartMonkey" / "DartMonkey.json").write_text(json.dumps(tower(attack(projectile()))))
    (dump / "towers" / "DartMonkey" / "DartMonkey-100.json").write_text(
        json.dumps(tower(attack(projectile(pierce=3)), applied=["Sharp Shots"])))
    (dump / "upgrades" / "Sharp Shots.json").write_text(json.dumps({"name": "Sharp Shots", "cost": 140.0}))
    wiki = tmp_path / "wiki"
    wiki.mkdir()
    a = {"name": "dart", "damage": 1, "pierce": 2, "cooldown": 0.95}
    (wiki / "dart_monkey.json").write_text(json.dumps({
        "name": "Dart Monkey", "cost": 200,
        "base": {"range": 32, "camo": False, "attacks": [a]},
        "paths": {"top": [{"tier": 1, "name": "Sharp Shots", "cost": 150,
                           "state": {"range": 32, "camo": False, "attacks": [dict(a, pierce=3)]}}]},
    }))
    report = cdw.compare(dump, wiki)
    assert "| cost | 1 | 2 |" in report  # 150 im Wiki, 140 im Spiel
    assert "| range | 2 | 2 |" in report
    assert "| attack | 2 | 2 |" in report
    assert "Kosten Wiki 150 / Spiel 140" in report
