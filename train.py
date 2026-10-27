"""MaskablePPO auf der BTD-Simulation trainieren.

    python train.py --steps 2_000_000 --envs 64                 # Rust-Engine (Standard)
    python train.py --steps 200_000 --envs 12 --engine python   # Python-Referenz
    tensorboard --logdir runs

Die Rust-Engine braucht die Bindings: maturin develop --release -m engine/crates/btd-py/Cargo.toml
"""
import argparse

from sb3_contrib import MaskablePPO
from stable_baselines3.common.callbacks import CheckpointCallback
from stable_baselines3.common.env_util import make_vec_env
from stable_baselines3.common.vec_env import SubprocVecEnv

from btd_sim import BTDEnv

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--steps", type=int, default=1_000_000)
    ap.add_argument("--envs", type=int, default=8)
    ap.add_argument("--out", default="models/ppo_btd")
    ap.add_argument("--resume", default=None)
    ap.add_argument("--engine", choices=["rust", "python"], default="rust")
    args = ap.parse_args()

    if args.engine == "rust":
        from btd_sim.rust_env import RustVecEnv
        env = RustVecEnv(args.envs)
    else:
        env = make_vec_env(BTDEnv, n_envs=args.envs, vec_env_cls=SubprocVecEnv)
    if args.resume:
        model = MaskablePPO.load(args.resume, env=env)
    else:
        model = MaskablePPO(
            "MlpPolicy", env,
            n_steps=512, batch_size=1024, n_epochs=6,
            gamma=0.995, gae_lambda=0.95, ent_coef=0.01, learning_rate=3e-4,
            policy_kwargs=dict(net_arch=[512, 256]),
            tensorboard_log="runs", verbose=1,
        )
    model.learn(total_timesteps=args.steps,
                callback=CheckpointCallback(100_000 // args.envs, "models/checkpoints"),
                reset_num_timesteps=not args.resume)
    model.save(args.out)
    print("gespeichert:", args.out)
