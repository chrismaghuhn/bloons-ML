"""MaskablePPO auf der BTD-Simulation trainieren.

    python train.py --steps 2_000_000 --envs 64                 # Rust-Engine (Standard)
    python train.py --steps 200_000 --envs 12 --engine python   # Python-Referenz
    python train.py --data data/local/game_v1_tutorial_medium.json --preview 3   # Spieldaten (M2)
    python train.py --data data/local/game_v1_tutorial_medium.json --preview 3 --arch conv   # Faltungs-Policy
    tensorboard --logdir runs

Die Rust-Engine braucht die Bindings: maturin develop --release -m engine/crates/btd-py/Cargo.toml
"""
import argparse
from collections import deque
from pathlib import Path

import numpy as np
from sb3_contrib import MaskablePPO
from stable_baselines3.common.callbacks import BaseCallback, CheckpointCallback
from stable_baselines3.common.env_util import make_vec_env
from stable_baselines3.common.vec_env import SubprocVecEnv

from btd_sim import BTDEnv


class RoundLogger(BaseCallback):
    """Loggt die erreichte Runde beendeter Episoden (Mittel und Maximum der letzten 100)."""

    def __init__(self):
        super().__init__()
        self.rounds = deque(maxlen=100)

    def _on_step(self):
        for info in self.locals["infos"]:
            if "round" in info and "episode" in info:
                self.rounds.append(info["round"])
        return True

    def _on_rollout_end(self):
        if self.rounds:
            self.logger.record("btd/round_mean", float(np.mean(self.rounds)))
            self.logger.record("btd/round_max", max(self.rounds))

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--steps", type=int, default=1_000_000)
    ap.add_argument("--envs", type=int, default=8)
    ap.add_argument("--out", default="models/ppo_btd")
    ap.add_argument("--resume", default=None)
    ap.add_argument("--engine", choices=["rust", "python"], default="rust")
    ap.add_argument("--data", default=None, help="game_v1-Datei (Standard: sim_v0)")
    ap.add_argument("--preview", type=int, default=0, help="Runden-Vorschau in der Observation (nur Rust)")
    ap.add_argument("--device", default="auto")
    ap.add_argument("--arch", choices=["mlp", "conv"], default="mlp", help="conv: btd_sim.policy.GridPolicy (nur Rust)")
    ap.add_argument("--conv-width", type=int, default=32, help="Kanäle der Faltungs-Policy")
    ap.add_argument("--conv-blocks", type=int, default=4, help="Residual-Blöcke der Faltungs-Policy")
    ap.add_argument("--seed", type=int, default=None)
    args = ap.parse_args()

    if args.engine == "rust":
        from btd_sim.rust_env import DEFAULT_DATA, RustVecEnv
        env = RustVecEnv(args.envs, args.data or DEFAULT_DATA, preview_rounds=args.preview)
    else:
        if args.data or args.preview:
            ap.error("--data/--preview gibt es nur mit --engine rust")
        env = make_vec_env(BTDEnv, n_envs=args.envs, vec_env_cls=SubprocVecEnv)
    if args.resume:
        model = MaskablePPO.load(args.resume, env=env, device=args.device)
    else:
        if args.arch == "conv":
            if args.engine != "rust":
                ap.error("--arch conv gibt es nur mit --engine rust")
            from btd_sim.policy import GridPolicy, grid_spec
            policy = GridPolicy
            grid = grid_spec(args.data or DEFAULT_DATA, env.observation_space.shape[0], env.action_space.n)
            policy_kwargs = dict(grid=grid, width_ch=args.conv_width, blocks=args.conv_blocks)
        else:
            policy, policy_kwargs = "MlpPolicy", dict(net_arch=[512, 256])
        model = MaskablePPO(
            policy, env,
            n_steps=512, batch_size=1024, n_epochs=6,
            gamma=0.995, gae_lambda=0.95, ent_coef=0.01, learning_rate=3e-4,
            policy_kwargs=policy_kwargs, seed=args.seed,
            tensorboard_log="runs", verbose=1, device=args.device,
        )
    model.learn(total_timesteps=args.steps,
                callback=[CheckpointCallback(100_000 // args.envs, "models/checkpoints", name_prefix=Path(args.out).name), RoundLogger()],
                reset_num_timesteps=not args.resume, tb_log_name=f"{args.arch}")
    model.save(args.out)
    print("gespeichert:", args.out)
