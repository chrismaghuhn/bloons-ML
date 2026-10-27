"""gymnasium-/sb3-Wrapper um die Rust-Engine (`btd_rs`, gebaut mit maturin).

    maturin develop --release -m engine/crates/btd-py/Cargo.toml            # schnell (f32)
    maturin develop --release -m engine/crates/btd-py/Cargo.toml --features f64   # bit-genau zu btd_sim

`RustBTDEnv` hat dieselbe API wie `btd_sim.env.BTDEnv` (Aktionen, Observation, Rewards,
`action_masks`). `RustVecEnv` ist eine sb3-VecEnv, bei der alle Envs in einem Rust-Aufruf
parallel laufen; MaskablePPO holt die Masken über `env_method("action_masks")`.
"""
from __future__ import annotations

import time
from pathlib import Path

import gymnasium as gym
import numpy as np
from gymnasium import spaces
from stable_baselines3.common.vec_env.base_vec_env import VecEnv

import btd_rs

DEFAULT_DATA = str(Path(__file__).resolve().parent.parent / "data" / "sim_v0.json")


class RustBTDEnv(gym.Env):
    metadata = {"render_modes": []}

    def __init__(self, data_path: str = DEFAULT_DATA, record: bool = False, **cfg):
        self.env = btd_rs.Env(data_path, record=record, **cfg)
        self.action_space = spaces.Discrete(self.env.n_actions)
        self.observation_space = spaces.Box(0.0, 1.0, shape=(self.env.obs_size,), dtype=np.float32)

    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        obs = self.env.reset()
        return obs, self._info()

    def step(self, action):
        obs, reward, terminated, info = self.env.step(int(action))
        return obs, reward, terminated, False, info

    def action_masks(self) -> np.ndarray:
        return self.env.action_masks()

    def _info(self):
        e = self.env
        return {"round": e.round, "lives": e.lives, "cash": e.cash, "towers": e.towers}

    # Spielprotokolle
    def save_replay(self, path: str, last: bool = False):
        self.env.save_replay(path, last)

    def load_replay(self, path: str, until_round: int | None = None, strict: bool = True):
        return self.env.load_replay(path, until_round, strict)


class RustVecEnv(VecEnv):
    """sb3-VecEnv über `btd_rs.VecEnv` (Auto-Reset, Episoden-Statistik für das sb3-Logging)."""

    def __init__(self, n_envs: int, data_path: str = DEFAULT_DATA, **cfg):
        self.venv = btd_rs.VecEnv(data_path, n_envs, **cfg)
        obs_space = spaces.Box(0.0, 1.0, shape=(self.venv.obs_size,), dtype=np.float32)
        super().__init__(n_envs, obs_space, spaces.Discrete(self.venv.n_actions))
        self._actions = np.zeros(n_envs, dtype=np.int64)
        self._ep_ret = np.zeros(n_envs)
        self._ep_len = np.zeros(n_envs, dtype=np.int64)
        self._t0 = time.time()

    def reset(self):
        self._ep_ret[:] = 0
        self._ep_len[:] = 0
        return self.venv.reset()

    def step_async(self, actions):
        self._actions = np.ascontiguousarray(actions, dtype=np.int64).reshape(self.num_envs)

    def step_wait(self):
        obs, rewards, dones, terminal_obs, rounds = self.venv.step(self._actions)
        self._ep_ret += rewards
        self._ep_len += 1
        infos = [{} for _ in range(self.num_envs)]
        for i in np.flatnonzero(dones):
            infos[i] = {
                "terminal_observation": terminal_obs[i],
                "TimeLimit.truncated": False,
                "episode": {"r": float(self._ep_ret[i]), "l": int(self._ep_len[i]),
                            "t": round(time.time() - self._t0, 6)},
                "round": int(rounds[i]),
            }
            self._ep_ret[i] = 0
            self._ep_len[i] = 0
        return obs, rewards.astype(np.float32), dones, infos

    def action_masks(self) -> np.ndarray:
        return self.venv.action_masks()

    def env_method(self, method_name, *args, indices=None, **kwargs):
        if method_name != "action_masks":
            raise NotImplementedError(f"RustVecEnv.env_method({method_name!r})")
        masks = self.venv.action_masks()
        return list(masks[self._indices(indices)])

    def get_attr(self, attr_name, indices=None):
        n = len(self._indices(indices))
        if attr_name == "render_mode":
            return [None] * n
        if attr_name == "action_masks":  # sb3_contrib prüft so, ob Masken unterstützt werden
            return [self.action_masks] * n
        raise AttributeError(f"RustVecEnv hat kein Attribut {attr_name!r} pro Env")

    def set_attr(self, attr_name, value, indices=None):
        raise AttributeError(f"RustVecEnv.set_attr({attr_name!r}) wird nicht unterstützt")

    def env_is_wrapped(self, wrapper_class, indices=None):
        return [False] * len(self._indices(indices))

    def close(self):
        pass

    def _indices(self, indices):
        if indices is None:
            return list(range(self.num_envs))
        return [indices] if isinstance(indices, int) else list(indices)
