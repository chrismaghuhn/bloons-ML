"""Modell (oder Zufalls-Agent) spielen lassen und erreichte Runden ausgeben.

    python evaluate.py --random
    python evaluate.py --model models/ppo_btd --show
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
    if show:
        print(env.render())
    return info["round"], info["lives"]


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--model")
    ap.add_argument("--random", action="store_true")
    ap.add_argument("--episodes", type=int, default=10)
    ap.add_argument("--show", action="store_true")
    args = ap.parse_args()

    env = BTDEnv()
    rng = np.random.default_rng(0)
    if args.random:
        policy = lambda obs, mask: rng.choice(np.flatnonzero(mask))
    else:
        from sb3_contrib import MaskablePPO
        model = MaskablePPO.load(args.model)
        policy = lambda obs, mask: model.predict(obs, action_masks=mask, deterministic=True)[0]

    results = [play(env, policy, args.show) for _ in range(args.episodes)]
    rounds = [r for r, _ in results]
    print(f"Runden: Ø {np.mean(rounds):.1f}  max {max(rounds)}  Siege {sum(l > 0 for _, l in results)}/{len(results)}")
