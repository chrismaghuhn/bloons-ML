"""Modell (oder Zufalls-Agent) spielen lassen und erreichte Runden ausgeben.

    python evaluate.py --random
    python evaluate.py --model models/ppo_btd --show
    python evaluate.py --model models/exp_conv --data data/local/game_v1_tutorial_medium.json --preview 3
    python evaluate.py --model models/r60_bans --data … --preview 3 --ban-towers 2 --per-ban   # je Sperre
"""
import argparse

import numpy as np

from btd_sim import BTDEnv


def play(env, policy, show=False, banned=None):
    obs, info = env.reset(options={"banned": banned} if banned is not None else None)
    done = False
    while not done:
        a = policy(obs, env.action_masks())
        obs, r, done, _, info = env.step(a)
    if show and isinstance(env, BTDEnv):  # nur die Python-Referenz kann rendern
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
    ap.add_argument("--ban-towers", type=int, default=0, help="wie beim Training (Observation muss passen)")
    ap.add_argument("--per-ban", action="store_true", help="je einen Turmtyp sperren und getrennt auswerten")
    args = ap.parse_args()

    if args.data:
        from btd_sim.rust_env import RustBTDEnv
        env = RustBTDEnv(args.data, preview_rounds=args.preview, max_banned_towers=args.ban_towers)
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

    def report(label, banned=None):
        results = [play(env, policy, args.show, banned) for _ in range(args.episodes)]
        rounds = [r for r, _ in results]
        print(f"{label}Runden: Ø {np.mean(rounds):.1f}  max {max(rounds)}  "
              f"Siege {sum(l > 0 for _, l in results)}/{len(results)}")

    if args.per_ban:
        if not args.data:
            ap.error("--per-ban braucht --data")
        import json
        with open(args.data) as f:
            names = [t["name"] for t in json.load(f)["towers"]]
        report(f"{'keine Sperre':<16}", [False] * len(names))
        for i, name in enumerate(names):
            report(f"{'ohne ' + name:<16}", [j == i for j in range(len(names))])
    else:
        report("")
