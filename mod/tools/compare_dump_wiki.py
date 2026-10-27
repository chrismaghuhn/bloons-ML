"""Plausibilitätsprüfung: Spiel-Dump (data/raw/game_dump/<version>) gegen die Wiki-Daten (data/base/towers).

Vergleicht pro Turm:
  - Basispreis und Upgrade-Kosten (über den Upgrade-Namen)
  - Reichweite und Camo-Erkennung der Basis und jedes reinen Pfad-Tiers (X-0-0, 0-X-0, 0-0-X)
  - jede Wiki-Attacke (damage/pierce/cooldown, ggf. blast_radius) gegen alle schadensverursachenden
    Projektile im Dump; "Treffer" = es gibt ein Dump-Projektil mit genau diesen Werten.

Ändert weder Dump noch Wiki-Daten; schreibt nur den Bericht nach <dump>/compare_wiki.md (im gitignored Dump-Ordner).

    .venv/bin/python mod/tools/compare_dump_wiki.py [--dump data/raw/game_dump/57.4] [--wiki data/base/towers]
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PATHS = ("top", "middle", "bottom")


def short_type(obj) -> str:
    t = obj.get("$type", "") if isinstance(obj, dict) else ""
    return t.split(",")[0].rsplit(".", 1)[-1]


def latest_dump(root: Path) -> Path:
    if not root.is_dir():
        raise SystemExit(f"kein Dump-Ordner {root} (in einem Worktree --dump angeben)")
    # .tmp-*/.old-* sind Reste eines laufenden oder abgebrochenen Dumps der Mod
    versions = sorted(
        (p for p in root.iterdir() if p.is_dir() and ".tmp-" not in p.name and ".old-" not in p.name),
        key=lambda p: [(int(x), "") if x.isdigit() else (-1, x) for x in p.name.split(".")],
    )
    if not versions:
        raise SystemExit(f"kein Dump unter {root}")
    return versions[-1]


# ------------------------------------------------------------------ Dump auswerten
def projectile_entries(proj: dict, rate: float, range_: float, path: str, out: list):
    """Alle Projektile (inkl. Sub-Projektile aus Behaviors) mit DamageModel sammeln."""
    if not isinstance(proj, dict):
        return
    behaviors = proj.get("behaviors") or []
    dmg = next((b for b in behaviors if short_type(b) == "DamageModel"), None)
    travel = next((b for b in behaviors if short_type(b).startswith("TravelStrait")), None)
    mods = [
        {"tags": b.get("tags") or b.get("tag"), "mult": b.get("damageMultiplier"), "add": b.get("damageAddative")}
        for b in behaviors if short_type(b).startswith("DamageModifierFor")
    ]
    name = f"{path}/{proj.get('id') or short_type(proj)}"
    if dmg is not None:
        out.append({
            "where": name,
            "damage": dmg.get("damage"),
            "pierce": proj.get("pierce"),
            "radius": proj.get("radius"),
            "rate": rate,
            "range": range_,
            "immune": dmg.get("immuneBloonProperties"),
            "speed": travel.get("speed") if travel else None,
            "lifespan": travel.get("lifespan") if travel else None,
            "modifiers": mods,
        })
    for b in behaviors:
        for key in ("projectile", "projectileModel"):
            if isinstance(b, dict) and isinstance(b.get(key), dict):
                projectile_entries(b[key], rate, range_, name, out)


def attacks_of(tower: dict) -> list[dict]:
    return [b for b in tower.get("behaviors") or [] if short_type(b).startswith("Attack") and "weapons" in b]


def dump_entries(tower: dict) -> list[dict]:
    out: list[dict] = []
    for a in attacks_of(tower):
        for w in a.get("weapons") or []:
            projectile_entries(w.get("projectile"), w.get("rate"), a.get("range"), a.get("name", "attack"), out)
    return out


def dump_camo(tower: dict) -> bool | None:
    """Camo der Hauptattacke = erste Attacke mit DamageModel; Camo, wenn sie keinen aktiven FilterInvisible*-Filter
    hat. Attacken ohne Schaden (Buffs, Effekte) zählen nicht. None, wenn der Turm selbst keine solche Attacke hat
    (Village ohne Ballista, Beast Handler: Camo kommt dort über Buffs bzw. Sub-Tower und ist so nicht prüfbar)."""
    for a in attacks_of(tower):
        entries: list[dict] = []
        for w in a.get("weapons") or []:
            projectile_entries(w.get("projectile"), w.get("rate"), a.get("range"), "", entries)
        if not entries:
            continue
        filters = []
        for b in a.get("behaviors") or []:
            if short_type(b) == "AttackFilterModel":
                filters += b.get("filters") or []
        return not any(short_type(f).startswith("FilterInvisible") and f.get("isActive") for f in filters)
    return None


# ------------------------------------------------------------------ Vergleich
def close(a, b, tol=1e-3) -> bool:
    if a is None or b is None:
        return False
    try:
        return math.isclose(float(a), float(b), rel_tol=tol, abs_tol=tol)
    except (TypeError, ValueError):
        return False


def match_attack(wiki_attack: dict, entries: list[dict]) -> tuple[str, dict | None]:
    keys = [k for k in ("damage", "pierce", "cooldown") if isinstance(wiki_attack.get(k), (int, float))]
    # damage 0 (Buffs, Kleber, Effekte) hat im Spiel kein DamageModel-Projektil als Gegenstück: wie ohne damage
    if wiki_attack.get("damage") == 0:
        keys.remove("damage")
    if not keys:
        return "skip", None

    def score(e):
        s = 0
        for k in keys:
            s += close(wiki_attack[k], e["rate"] if k == "cooldown" else e[k])
        return s

    best = max(entries, key=score, default=None)
    if best is None:
        return "no-dump-attack", None
    s = score(best)
    if "damage" not in keys:
        # ohne damage kann jede beliebige Nebenattacke mit gleichem pierce/cooldown passen: nicht zählen
        return ("weak" if s == len(keys) else "weak-mismatch"), best
    if s == len(keys):
        return "match", best
    return f"{s}/{len(keys)}", best


def fmt(v):
    if isinstance(v, float):
        return f"{v:g}"
    return str(v)


def compare(dump: Path, wiki_dir: Path) -> str:
    upgrades = {}
    for f in (dump / "upgrades").glob("*.json"):
        u = json.loads(f.read_text())
        upgrades[u.get("name")] = u
    lines = [f"# Dump {dump.name} vs. Wiki ({wiki_dir})", ""]
    totals = {"cost": [0, 0], "range": [0, 0], "camo": [0, 0], "attack": [0, 0]}
    weak = 0  # Wiki ohne damage, pierce/cooldown gleich; nicht in "attack" gezählt
    weak_mismatch = 0  # Wiki ohne damage, nicht gleich; in der Liste, nicht in "attack" gezählt
    no_dump = 0  # Attacke steckt in einem Sub-Tower (z.B. Beast-Handler-Tiere), nicht in diesem TowerModel
    diffs: list[str] = []

    for wf in sorted(wiki_dir.glob("*.json")):
        w = json.loads(wf.read_text())
        base_id = w["name"].replace(" ", "")
        tdir = dump / "towers" / base_id
        if not tdir.exists():
            diffs.append(f"- **{w['name']}**: kein Turm `{base_id}` im Dump")
            continue

        def load(tiers):
            name = base_id if tiers == (0, 0, 0) else f"{base_id}-{''.join(map(str, tiers))}"
            p = tdir / f"{name}.json"
            return json.loads(p.read_text()) if p.exists() else None

        states = [((0, 0, 0), "Basis", w.get("cost"), w.get("base") or {})]
        for pi, path in enumerate(PATHS):
            for up in (w.get("paths") or {}).get(path, []):
                tiers = [0, 0, 0]
                tiers[pi] = up["tier"]
                states.append((tuple(tiers), f"{path} {up['tier']} {up['name']}", up.get("cost"), up.get("state") or {}, up["name"]))

        for st in states:
            tiers, label, wiki_cost, state = st[:4]
            t = load(tiers)
            tag = f"{w['name']} {''.join(map(str, tiers))} ({label})"
            if t is None:
                diffs.append(f"- {tag}: fehlt im Dump")
                continue
            # Kosten: Basis = TowerModel.cost, Tiers = UpgradeModel.cost
            # Upgrade-Namen im Spiel weichen vom Wiki ab ("Buccaneer-Faster Shooting", "Large Calibre"):
            # das zuletzt angewendete Upgrade des Turms ist das gesuchte
            applied = t.get("appliedUpgrades") or []
            dump_cost = t.get("cost") if tiers == (0, 0, 0) else (upgrades.get(applied[-1] if applied else None) or {}).get("cost")
            if wiki_cost is not None:
                totals["cost"][1] += 1
                if close(wiki_cost, dump_cost):
                    totals["cost"][0] += 1
                else:
                    diffs.append(f"- {tag}: Kosten Wiki {fmt(wiki_cost)} / Spiel {fmt(dump_cost)}")
            if isinstance(state.get("range"), (int, float)):
                totals["range"][1] += 1
                if close(state["range"], t.get("range")):
                    totals["range"][0] += 1
                else:
                    diffs.append(f"- {tag}: Reichweite Wiki {fmt(state['range'])} / Spiel {fmt(t.get('range'))}")
            if isinstance(state.get("camo"), bool) and dump_camo(t) is not None:
                totals["camo"][1] += 1
                if state["camo"] == dump_camo(t):
                    totals["camo"][0] += 1
                else:
                    diffs.append(f"- {tag}: Camo Wiki {state['camo']} / Spiel {dump_camo(t)}")
            entries = dump_entries(t)
            for a in state.get("attacks") or []:
                result, best = match_attack(a, entries)
                if result == "skip":
                    continue
                if result == "weak":
                    weak += 1
                    continue
                if result == "weak-mismatch":
                    weak_mismatch += 1
                if result == "no-dump-attack":
                    no_dump += 1
                    diffs.append(f"- {tag} `{a.get('name')}`: keine Attacke im TowerModel (Sub-Tower?)")
                    continue
                if result != "weak-mismatch":
                    totals["attack"][1] += 1
                if result == "match":
                    totals["attack"][0] += 1
                    continue
                wiki_s = ", ".join(f"{k}={fmt(a.get(k))}" for k in ("damage", "pierce", "cooldown") if a.get(k) is not None)
                dump_s = "–" if best is None else f"damage={fmt(best['damage'])}, pierce={fmt(best['pierce'])}, cooldown={fmt(best['rate'])} ({best['where']})"
                diffs.append(f"- {tag} `{a.get('name')}`: Wiki {wiki_s} / Spiel bestes {dump_s}")

    lines.append("| Prüfung | gleich | geprüft |")
    lines.append("|---|---|---|")
    for k, (ok, n) in totals.items():
        lines.append(f"| {k} | {ok} | {n} |")
    lines.append("")
    lines.append(f"Nicht in `attack` gezählt: Wiki-Attacken ohne damage ({weak} mit gleichem pierce/cooldown, "
                 f"{weak_mismatch} abweichend, siehe Liste; damage 0 zählt als ohne damage), "
                 f"{no_dump} ohne Attacke im TowerModel (Sub-Tower, z.B. Beast-Handler-Tiere).")
    lines += ["", "## Abweichungen", ""] + (diffs or ["keine"])
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dump", type=Path, help="Dump-Ordner einer Version (Standard: neueste unter data/raw/game_dump)")
    ap.add_argument("--wiki", type=Path, default=REPO / "data" / "base" / "towers")
    args = ap.parse_args()
    dump = args.dump or latest_dump(REPO / "data" / "raw" / "game_dump")
    report = compare(dump, args.wiki)
    out = dump / "compare_wiki.md"
    out.write_text(report)
    head = report.split("## Abweichungen")[0]
    print(head)
    print(f"Bericht: {out}")


if __name__ == "__main__":
    main()
