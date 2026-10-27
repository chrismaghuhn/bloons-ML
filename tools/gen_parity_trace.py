"""Erzeugt den Golden-Trace für den Rust-Paritätstest (engine/crates/btd-core/tests/parity.rs).

Zufällige, aber feste (geseedete) Aktionsfolgen laufen durch die Python-Referenz-Sim. Pro
Runde werden die versuchten Aktionen, ob sie akzeptiert wurden, und der komplette Zustand
danach festgehalten; Floats als IEEE-754-Bitmuster, damit der Vergleich bit-genau ist.

    python tools/gen_parity_trace.py
"""
import json
import random
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from btd_sim import engine as e  # noqa: E402

OUT = ROOT / "engine" / "crates" / "btd-core" / "tests" / "data" / "parity_trace.json"
EPISODES = 12
SEED = 2026


def bits(x: float) -> int:
    return struct.unpack("<Q", struct.pack("<d", float(x)))[0]


def snapshot(g: e.Game, report: dict) -> dict:
    return {
        "cash": g.cash, "lives": g.lives, "round": g.round,
        "leaked": report["leaked"], "pops": report["pops"], "seconds": bits(report["seconds"]),
        "towers": [
            [t.kind, t.x, t.y, t.tiers[0], t.tiers[1], t.spent, bits(t.range), bits(t.cooldown),
             t.damage, t.pierce, t.dmg_type, bits(t.blast)]
            for t in g.towers.values()
        ],
    }


def random_actions(g: e.Game, rng: random.Random) -> list:
    actions = []
    for _ in range(rng.randint(0, 6)):
        r = rng.random()
        cells = list(g.towers)
        if r < 0.5 or not cells:
            a = ["place", rng.randrange(len(e.TOWERS)), rng.randrange(e.GRID_W), rng.randrange(e.GRID_H)]
            ok = g.place(a[1], a[2], a[3])
        elif r < 0.9:
            x, y = rng.choice(cells) if rng.random() < 0.8 else (rng.randrange(e.GRID_W), rng.randrange(e.GRID_H))
            a = ["upgrade", x, y, rng.randrange(2)]
            ok = g.upgrade(x, y, a[3])
        else:
            x, y = rng.choice(cells)
            a = ["sell", x, y]
            ok = g.sell(x, y)
        actions.append({"action": a, "ok": ok})
    return actions


def main():
    rng = random.Random(SEED)
    episodes = []
    for _ in range(EPISODES):
        g = e.Game()
        g.cash = rng.choice([650, 2000, 6000])  # mehr Cash = mehr Türme/Upgrades im Trace
        start_cash = g.cash
        rounds = []
        while not g.over:
            actions = random_actions(g, rng)
            report = g.play_round()
            rounds.append({"actions": actions, "after": snapshot(g, report)})
        episodes.append({"start_cash": start_cash, "rounds": rounds})
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({"format": "parity_trace/1", "data": "data/sim_v0.json", "episodes": episodes},
                              separators=(",", ":")) + "\n")
    n = sum(len(ep["rounds"]) for ep in episodes)
    print(f"{OUT.relative_to(ROOT)}: {EPISODES} Episoden, {n} Runden, {OUT.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
