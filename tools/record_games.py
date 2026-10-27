"""Spiele eines Modells als Replays aufzeichnen (z.B. als Quelle B für Startzustände).

    python tools/record_games.py models/r60_base --data data/local/game_v1_tutorial_medium60.json \\
        --preview 3 --episodes 50 --out data/local/starts/r60_base

Stochastische Policy; mit `--ban-towers` (wie beim Training des Modells) werden die Sperren je Spiel
gewürfelt, das ergibt unterschiedliche Builds.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from btd_sim.rust_env import RustBTDEnv  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("model")
    ap.add_argument("--data", required=True)
    ap.add_argument("--preview", type=int, default=0)
    ap.add_argument("--ban-towers", type=int, default=0)
    ap.add_argument("--episodes", type=int, default=50)
    ap.add_argument("--seed", type=int, default=12345, help="eigener Seed, getrennt vom Training")
    ap.add_argument("--out", required=True, help="Verzeichnis für die .jsonl-Dateien")
    args = ap.parse_args()

    from sb3_contrib import MaskablePPO
    model = MaskablePPO.load(args.model, device="cuda")
    env = RustBTDEnv(args.data, record=True, preview_rounds=args.preview,
                     max_banned_towers=args.ban_towers, seed=args.seed)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    rounds = []
    for i in range(args.episodes):
        obs, info = env.reset()
        done = False
        while not done:
            obs, _, done, _, info = env.step(model.predict(obs, action_masks=env.action_masks())[0])
        (out / f"{i:04d}.jsonl").write_text(env.env.replay_jsonl())
        rounds.append(info["round"])
    print(f"{out}: {len(rounds)} Spiele, Ø Runde {sum(rounds) / len(rounds):.1f}")


if __name__ == "__main__":
    main()
