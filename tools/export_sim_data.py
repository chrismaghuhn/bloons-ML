"""Exportiert die Daten der Python-Referenz-Sim (btd_sim) nach data/sim_v0.game_v1.json.

Der Datensatz im Format `game_v1` (docs/m2-plan.md) ist die Grundlage für die Parität der
Rust-Engine zur Python-Sim. So gibt es nur eine Quelle für Karte, Bloons, Türme und Runden, und
Rust muss weder das Pfad-Raster noch den Runden-Generator nachbauen.

    python tools/export_sim_data.py           # schreiben
    python tools/export_sim_data.py --check   # nur prüfen, ob die Datei aktuell ist
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from btd_sim import engine as e  # noqa: E402

OUT_V1 = ROOT / "data" / "sim_v0.game_v1.json"


def grid(mask):
    return ["".join("#" if v else "." for v in row) for row in mask]


def build() -> dict:
    """Zwischenstand im alten sim_v0-Aufbau (Raster als Zeichenketten), Basis für build_v1."""
    track = e.Track(e.MONKEY_MEADOW)
    return {
        "format": "sim_v0",
        "rules": {
            "dt": e.DT,
            "cd_eps": e.CD_EPS,
            "max_round_seconds": e.Game.MAX_ROUND_SECONDS,
            "child_spacing": e.CHILD_SPACING,
            "start_cash": e.Game.START_CASH,
            "start_lives": e.Game.START_LIVES,
            "sell_ratio": e.SELL_RATIO,
            "round_bonus_base": e.ROUND_BONUS_BASE,
            "max_tier": e.MAX_TIER,
            "max_second_path": e.MAX_SECOND_PATH,
        },
        "map": {
            "name": "monkey_meadow",
            "grid_w": e.GRID_W,
            "grid_h": e.GRID_H,
            "points": track.pts.tolist(),
            # kumulierte Längen mitliefern statt in Rust neu zu rechnen (np.hypot != sqrt bei Diagonalen)
            "cum": track.cum.tolist(),
            "path_mask": grid(track.path_mask),
            "buildable": grid(track.buildable),
        },
        "bloons": [
            {"name": n, "children": list(e.BLOONS[n][0]), "speed": float(e.SPEED[i]),
             "rbe": int(e.RBE[i]), "immune": list(e.BLOONS[n][3])}
            for i, n in enumerate(e.BLOON_NAMES)
        ],
        "towers": [
            {"name": t.name, "cost": t.cost, "range": t.range, "cooldown": t.cooldown,
             "damage": t.damage, "pierce": t.pierce, "dmg_type": t.dmg_type, "blast": t.blast,
             "upgrades": [[{"cost": c, "effects": fx} for c, fx in path] for path in t.upgrades]}
            for t in e.TOWERS
        ],
        "rounds": [[[t, e.BLOON_NAMES[k]] for t, k in r] for r in e.ROUNDS],
    }


# Schadenstyp -> Bloon-Eigenschaften, die er nicht beschädigen kann (wie BTD6 "immuneBloonProperties")
CANNOT_DAMAGE = {"sharp": ["lead"], "explosive": ["black"], "normal": []}
# Bloon -> Eigenschaften; zusammen mit CANNOT_DAMAGE ergibt das genau die Immunitäten der Python-Sim
PROPS = {"black": ["black"], "white": ["white"], "lead": ["lead"], "zebra": ["black", "white"]}


def children_counts(children):
    """("pink", "pink") -> [["pink", 2]]; Reihenfolge bleibt erhalten."""
    out = []
    for c in children:
        if out and out[-1][0] == c:
            out[-1][1] += 1
        else:
            out.append([c, 1])
    return out


def variants(t: e.TowerType):
    """Alle gültigen Upgrade-Kombinationen, Werte in derselben Rechenreihenfolge wie Tower.apply."""
    out = []
    for t0 in range(e.MAX_TIER + 1):
        for t1 in range(e.MAX_TIER + 1):
            if t0 > e.MAX_SECOND_PATH and t1 > e.MAX_SECOND_PATH:
                continue
            tw = e.Tower.build(0, 0, 0)  # Werte des Dart werden gleich überschrieben
            tw.range, tw.cooldown, tw.damage, tw.pierce = t.range, t.cooldown, t.damage, t.pierce
            tw.dmg_type, tw.blast = t.dmg_type, t.blast
            for path, n in ((0, t0), (1, t1)):
                for tier in range(n):
                    cost, fx = t.upgrades[path][tier]
                    tw.apply(path, cost, fx)
            delivery = {"kind": "hitscan_splash", "radius": tw.blast} if tw.blast > 0 else {"kind": "hitscan"}
            out.append({
                "tiers": [t0, t1, 0], "range": tw.range, "footprint": 0.5,
                "attacks": [{"tag": t.name, "cooldown": tw.cooldown, "damage": tw.damage, "pierce": tw.pierce,
                             "cannot_damage": CANNOT_DAMAGE[tw.dmg_type], "camo": True, "delivery": delivery}],
            })
    return out


def build_v1() -> dict:
    """sim_v0 im Format game_v1: Raster-Koordinaten (1 Zelle = 1 Einheit), 1 HP pro Schicht, Hitscan."""
    v0 = build()
    track = e.Track(e.MONKEY_MEADOW)
    return {
        "format": "game_v1",
        "name": "sim_v0",
        "rules": {
            "tick_rate": 1.0 / e.DT, "cd_eps": e.CD_EPS, "max_round_seconds": e.Game.MAX_ROUND_SECONDS,
            "child_spacing": e.CHILD_SPACING, "start_cash": e.Game.START_CASH,
            "start_lives": e.Game.START_LIVES, "start_round": 1, "end_round": len(e.ROUNDS),
            "selling": True, "sell_ratio": e.SELL_RATIO, "cost_multiplier": 1.0, "cost_rounding": 1,
        },
        "map": {
            "name": "monkey_meadow_grid",
            "bounds": [0.0, 0.0, float(e.GRID_W), float(e.GRID_H)],
            "points": track.pts.tolist(),
            "cum": track.cum.tolist(),
            "grid": {"x0": 0.0, "y0": 0.0, "cell": 1.0, "w": e.GRID_W, "h": e.GRID_H,
                     "path": v0["map"]["path_mask"], "buildable": v0["map"]["buildable"]},
            "placement": "grid",
        },
        "bloons": [
            {"name": n, "hp": 1.0, "speed": float(e.SPEED[i]), "leak": int(e.RBE[i]),
             "children": children_counts(e.BLOONS[n][0]), "props": PROPS.get(n, [])}
            for i, n in enumerate(e.BLOON_NAMES)
        ],
        "towers": [
            {"name": t.name, "cost": t.cost,
             "upgrade_costs": [[c for c, _ in path] for path in t.upgrades] + [[]],
             "variants": variants(t)}
            for t in e.TOWERS
        ],
        # eine Gruppe pro Spawn (count 1): die Python-Sim hat explizite Spawn-Zeiten
        "rounds": [[{"bloon": e.BLOON_NAMES[k], "count": 1, "start": t, "end": t} for t, k in r]
                   for r in e.ROUNDS],
        "income": [{"cash_per_pop": 1.0, "end_bonus": float(e.ROUND_BONUS_BASE + r)}
                   for r in range(1, len(e.ROUNDS) + 1)],
    }


def render(data: dict) -> str:
    return json.dumps(data, indent=1) + "\n"


def main():
    outputs = [(OUT_V1, render(build_v1()))]
    if "--check" in sys.argv:
        stale = [o for o, text in outputs if not o.exists() or o.read_text() != text]
        if stale:
            sys.exit(f"veraltet: {', '.join(str(o.relative_to(ROOT)) for o in stale)}: "
                     "python tools/export_sim_data.py")
        print("sim_v0-Exporte sind aktuell")
        return
    for out, text in outputs:
        out.write_text(text)
        print(f"{out.relative_to(ROOT)} geschrieben")


if __name__ == "__main__":
    main()
