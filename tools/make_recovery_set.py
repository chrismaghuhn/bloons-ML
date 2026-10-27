"""Fester Eval-Satz "Erholung" (docs/design-start-states.md, Abschnitt 5): gestörte Zustände aus Replays.

    python tools/make_recovery_set.py data/local/eval/r60_base/*.jsonl \\
        --data data/local/game_v1_tutorial_medium60.json -n 50 -o data/local/eval/recovery_r60.json

Die Replays sollten nicht die sein, aus denen das Training seine Zustände (Quelle B) nimmt. Gezogen
wird gleichverteilt über die Runden (eigener Seed), gestört wie im Training. Ausgabe: JSON-Liste von
Zuständen; echte Spielstände von Hand können in dieselbe Liste (werden beim Aufbau geprüft).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import btd_rs  # noqa: E402

from btd_sim.starts import states_from_replay  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("replays", nargs="+")
    ap.add_argument("--data", required=True)
    ap.add_argument("-n", type=int, default=50)
    ap.add_argument("--seed", type=int, default=2026, help="Eval-Seed, getrennt vom Training")
    ap.add_argument("--min-round", type=int, default=3)
    ap.add_argument("-o", "--out", required=True)
    args = ap.parse_args()

    end = json.load(open(args.data))["rules"]["end_round"]
    states = [s for f in args.replays for s in states_from_replay(Path(f).read_text())
              if args.min_round <= s["round"] <= end - 3]
    rng = np.random.default_rng(args.seed)
    if not states:
        ap.error(f"keine Zustände in Runde {args.min_round}..{end - 3} in den Replays")
    by_round: dict[int, list] = {}
    for s in states:
        by_round.setdefault(s["round"], []).append(s)
    rounds = sorted(by_round)
    env = btd_rs.Env(args.data)
    out = []
    for i in range(args.n):
        r = rounds[rng.integers(len(rounds))]  # gleichverteilt über die Runden, nicht über die Zustände
        base = by_round[r][rng.integers(len(by_round[r]))]
        out.append(json.loads(env.perturb_state(json.dumps(base), args.seed * 1000 + i)))
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(json.dumps(out, indent=1))
    print(f"{args.out}: {len(out)} Zustände aus {len(states)} (Runden {min(rounds)}-{max(rounds)})")


if __name__ == "__main__":
    main()
