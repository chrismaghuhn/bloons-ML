"""Modell (oder Zufalls-Agent) spielen lassen und erreichte Runden ausgeben.

    python evaluate.py --random
    python evaluate.py --model models/ppo_btd --show
    python evaluate.py --model models/exp_conv --data data/local/game_v1_tutorial_medium.json --preview 3
"""
import argparse

import numpy as np

from btd_sim import BTDEnv


def play(env, policy, show=False):
    obs, info = env.reset()
    done = False
    while not done:
        a = policy(obs, env.action_masks())
        obs, r, done, _, info = env.step(a)
    if show and hasattr(env, "render"):
        print(env.render())
    return info["round"], info["lives"]


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--model")
    ap.add_argument("--random", action="store_true")
    ap.add_argument("--episodes", type=int, default=10)
    ap.add_argument("--show", action="store_true")
    ap.add_argument("--data", default=None, help="game_v1-Datei: Rust-Env statt Python-Referenz")
    ap.add_argument("--preview", type=int, default=0)
    ap.add_argument("--stochastic", action="store_true", help="Aktionen samplen statt argmax")
    args = ap.parse_args()

    if args.data:
        from btd_sim.rust_env import RustBTDEnv
        env = RustBTDEnv(args.data, preview_rounds=args.preview)
    else:
        env = BTDEnv()
    rng = np.random.default_rng(0)
    if args.random:
        policy = lambda obs, mask: rng.choice(np.flatnonzero(mask))
    else:
        from sb3_contrib import MaskablePPO
        model = MaskablePPO.load(args.model)
        det = not args.stochastic
        policy = lambda obs, mask: model.predict(obs, action_masks=mask, deterministic=det)[0]

    results = [play(env, policy, args.show) for _ in range(args.episodes)]
    rounds = [r for r, _ in results]
    print(f"Runden: Ø {np.mean(rounds):.1f}  max {max(rounds)}  Siege {sum(l > 0 for _, l in results)}/{len(results)}")
