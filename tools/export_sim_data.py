"""Exportiert die Daten der Python-Referenz-Sim (btd_sim) nach data/sim_v0.json.

Die Rust-Engine lädt diese Datei für den M1-Paritätsmodus. So gibt es nur eine Quelle für
Karte, Bloons, Türme und Runden, und Rust muss weder das Pfad-Raster noch den
Runden-Generator nachbauen.

    python tools/export_sim_data.py
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from btd_sim import engine as e  # noqa: E402

OUT = ROOT / "data" / "sim_v0.json"


def grid(mask):
    return ["".join("#" if v else "." for v in row) for row in mask]


def main():
    track = e.Track(e.MONKEY_MEADOW)
    data = {
        "format": "sim_v0",
        "rules": {
            "dt": e.DT,
            "cd_eps": e.CD_EPS,
            "max_round_seconds": e.Game.MAX_ROUND_SECONDS,
            "child_spacing": 0.15,
            "start_cash": e.Game.START_CASH,
            "start_lives": e.Game.START_LIVES,
            "sell_ratio": e.SELL_RATIO,
            "round_bonus_base": 100,
            "max_tier": e.MAX_TIER,
            "max_second_path": e.MAX_SECOND_PATH,
        },
        "map": {
            "name": "monkey_meadow",
            "grid_w": e.GRID_W,
            "grid_h": e.GRID_H,
            "points": track.pts.tolist(),
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
    OUT.write_text(json.dumps(data, indent=1) + "\n")
    print(f"{OUT.relative_to(ROOT)}: {len(data['rounds'])} Runden, {len(data['towers'])} Türme, "
          f"{len(data['bloons'])} Bloons")


if __name__ == "__main__":
    main()
