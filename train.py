"""MaskablePPO auf der BTD-Simulation trainieren.

    python train.py --steps 2_000_000 --envs 64                 # Rust-Engine (Standard)
    python train.py --steps 200_000 --envs 12 --engine python   # Python-Referenz
    python train.py --data data/local/game_v1_tutorial_medium.json --preview 3   # Spieldaten (M2)
    python train.py --data data/local/game_v1_tutorial_medium.json --preview 3 --arch mlp    # flaches MLP
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


class EntropySchedule(BaseCallback):
    """Entropie-Koeffizient linear von `start` auf `end` über `total` Schritte (ab dem Start dieses Laufs)."""

    def __init__(self, start: float, end: float, total: int):
        super().__init__()
        self.start, self.end, self.total = start, end, total
        self.first = None

    def _on_rollout_start(self):
        # self.num_timesteps wird erst in on_step aktualisiert, hier also vom Modell lesen
        t = self.model.num_timesteps
        if self.first is None:
            self.first = t
        progress = min(1.0, max(0.0, (t - self.first) / max(self.total, 1)))
        self.model.ent_coef = self.start + (self.end - self.start) * progress
        self.logger.record("train/ent_coef_schedule", self.model.ent_coef)

    def _on_step(self):
        return True


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
    ap.add_argument("--n-steps", type=int, default=None,
                    help="Schritte je Env und Rollout (neu: 512, Weitertrainieren: Wert des Modells)")
    ap.add_argument("--out", default="models/ppo_btd")
    ap.add_argument("--resume", default=None)
    ap.add_argument("--engine", choices=["rust", "python"], default="rust")
    ap.add_argument("--data", default=None, help="game_v1-Datei (Standard: sim_v0)")
    ap.add_argument("--preview", type=int, default=0, help="Runden-Vorschau in der Observation (nur Rust)")
    ap.add_argument("--device", default="auto")
    ap.add_argument("--reward-tower", type=float, default=0.0,
                    help="Reward je gebautem Turm, z.B. -0.5 für möglichst wenige Türme (nur Rust)")
    ap.add_argument("--arch", choices=["mlp", "conv"], default=None,
                    help="conv: btd_sim.policy.GridPolicy (Standard mit --engine rust), mlp: flaches MLP")
    ap.add_argument("--randomness", action="store_true",
                    help="Zufall im Spiel (z.B. Crit-Abstände) statt Mittelwerten, je Episode ein Seed (nur Rust)")
    ap.add_argument("--ban-towers", type=int, default=0,
                    help="je Episode bis zu so viele Turmtypen zufällig sperren (nur Rust)")
    ap.add_argument("--start-states", type=float, default=0.0,
                    help="Anteil der Episoden aus Startzuständen (docs/design-start-states.md), z.B. 0.3")
    ap.add_argument("--start-random", type=float, default=0.2,
                    help="davon zufällige Builds im Budget (Quelle C)")
    ap.add_argument("--start-replays", nargs="*", default=[],
                    help="Replays (.jsonl) als Quelle B und für die Budgetkurve")
    ap.add_argument("--ent-coef", type=float, default=0.01, help="Entropie-Koeffizient (Start)")
    ap.add_argument("--ent-coef-end", type=float, default=None,
                    help="wenn gesetzt: linear von --ent-coef auf diesen Wert über --steps (Wert je Rollout am "
                         "Rollout-Start; mit --resume beginnt der Verlauf wieder bei --ent-coef)")
    ap.add_argument("--conv-width", type=int, default=32, help="Kanäle der Faltungs-Policy")
    ap.add_argument("--conv-blocks", type=int, default=4, help="Residual-Blöcke der Faltungs-Policy")
    ap.add_argument("--amp", action=argparse.BooleanOptionalAction, default=True,
                    help="Faltungs-Policy in bf16 rechnen (nur CUDA, Update ~2x schneller)")
    ap.add_argument("--compile", action=argparse.BooleanOptionalAction, default=True,
                    help="Faltungs-Policy mit torch.compile übersetzen (nur CUDA)")
    ap.add_argument("--seed", type=int, default=None)
    args = ap.parse_args()
    if args.n_steps is not None and args.n_steps < 1:
        ap.error("--n-steps muss positiv sein")
    if args.arch is None:
        args.arch = "conv" if args.engine == "rust" else "mlp"
    if args.engine != "rust" and (args.data or args.preview or args.arch == "conv" or args.reward_tower
                                   or args.ban_towers or args.start_states or args.randomness):
        ap.error("--data/--preview/--arch conv/--reward-tower/--ban-towers/--start-states "
                 "gibt es nur mit --engine rust")
    if args.start_states and not args.start_replays:
        ap.error("--start-states braucht --start-replays (Quelle B und Budgetkurve)")
    if args.ent_coef < 0 or (args.ent_coef_end is not None and args.ent_coef_end < 0):
        ap.error("--ent-coef/--ent-coef-end müssen >= 0 sein")
    if args.resume:
        print("--resume: Architektur und Seed kommen aus dem Modell, --arch/--conv-*/--seed werden ignoriert "
              "(--amp/--compile/--ent-coef gelten)")

    if args.engine == "rust":
        from btd_sim.rust_env import DEFAULT_DATA, RustVecEnv
        env = RustVecEnv(args.envs, args.data or DEFAULT_DATA, preview_rounds=args.preview,
                         reward_tower=args.reward_tower, max_banned_towers=args.ban_towers,
                         seed=args.seed or 0, randomness=args.randomness)
    else:
        env = make_vec_env(BTDEnv, n_envs=args.envs, vec_env_cls=SubprocVecEnv)
    if args.resume:
        model = MaskablePPO.load(args.resume, env=env, device=args.device,
                                 custom_objects={} if args.n_steps is None else {"n_steps": args.n_steps})
        model.ent_coef = args.ent_coef  # sonst gälte still der gespeicherte Wert
    else:
        if args.arch == "conv":
            from btd_sim.policy import GridPolicy, grid_spec
            policy = GridPolicy
            grid = grid_spec(args.data or DEFAULT_DATA, env.observation_space.shape[0], env.action_space.n)
            policy_kwargs = dict(grid=grid, width_ch=args.conv_width, blocks=args.conv_blocks, amp=args.amp)
        else:
            policy, policy_kwargs = "MlpPolicy", dict(net_arch=[512, 256])
        model = MaskablePPO(
            policy, env,
            n_steps=args.n_steps or 512, batch_size=1024, n_epochs=6,
            gamma=0.995, gae_lambda=0.95, ent_coef=args.ent_coef, learning_rate=3e-4,
            policy_kwargs=policy_kwargs, seed=args.seed,
            tensorboard_log="runs", verbose=1, device=args.device,
        )
    from btd_sim.policy import GridNet
    net = model.policy.mlp_extractor
    if isinstance(net, GridNet):
        net.amp = args.amp  # auch beim Fortsetzen; ändert keine Gewichte
        if args.compile and model.device.type == "cuda":
            net.try_compile(model.device)
    callbacks = [CheckpointCallback(100_000 // args.envs, "models/checkpoints", name_prefix=Path(args.out).name),
                 RoundLogger()]
    if args.ent_coef_end is not None:
        callbacks.append(EntropySchedule(args.ent_coef, args.ent_coef_end, args.steps))
    if args.start_states:
        import json
        from btd_sim.starts import StartPool, StartStatesCallback, budget_curve, states_from_replay
        with open(args.data or DEFAULT_DATA) as f:
            game_data = json.load(f)
        pool, states_b = StartPool(), []
        for f in args.start_replays:
            states_b += states_from_replay(Path(f).read_text(), game_data["rules"]["end_round"])
        # nie mehr Leben als der Modus zum Start hat (Replays aus Medium mit 150 Leben für CHIMPS mit 1)
        for st in states_b:
            st["lives"] = min(st["lives"], game_data["rules"]["start_lives"])
        # Zustände aus Replays anderer Daten (z.B. 6 statt 12 Türme) können hier ungültig sein
        import btd_rs
        check, n_all = btd_rs.Env(args.data or DEFAULT_DATA), len(states_b)

        def buildable(st):
            try:
                check.reset_from(st if isinstance(st, str) else json.dumps(st))
                return True
            except ValueError:
                return False
        states_b = [st for st in states_b if buildable(st)]
        if not states_b:
            ap.error("--start-replays: kein Zustand passt zu diesen Daten (falsche Karte/Türme?)")
        if len(states_b) < n_all:
            print(f"Startzustände: {n_all - len(states_b)} von {n_all} passen nicht zu diesen Daten, verworfen")
        for st in states_b:
            pool.add(st, "B")
        budget = budget_curve(states_b, game_data)
        print(f"Startzustände: {len(states_b)} aus {len(args.start_replays)} Replays (Quelle B)")
        callbacks.append(StartStatesCallback(pool, budget, args.start_states, args.start_random, valid=buildable))
    model.learn(total_timesteps=args.steps,
                callback=callbacks,
                reset_num_timesteps=not args.resume, tb_log_name=Path(args.out).name)
    model.save(args.out)
    print("gespeichert:", args.out)
