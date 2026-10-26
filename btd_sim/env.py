"""Gymnasium-Environment um die Simulation, für MaskablePPO (sb3-contrib).

Ein Schritt = eine Entscheidung. Der Agent kann mehrere Türme bauen/upgraden
und startet dann mit Aktion 0 die nächste Runde, die komplett durchsimuliert wird.

Aktionsraum (Discrete), N = Anzahl Zellen:
    0                          -> Runde starten
    1 + k*N + c                -> Turm k auf Zelle c bauen
    1 + T*N + p*N + c          -> Turm auf Zelle c, Upgrade-Pfad p
    1 + T*N + 2*N + c          -> Turm auf Zelle c verkaufen
"""
from __future__ import annotations

import gymnasium as gym
import numpy as np
from gymnasium import spaces

from . import engine
from .engine import GRID_H, GRID_W, MAX_TIER, TOWERS, Game

N = GRID_W * GRID_H
T = len(TOWERS)
A_PLACE = 1
A_UPGRADE = A_PLACE + T * N
A_SELL = A_UPGRADE + 2 * N
N_ACTIONS = A_SELL + N

MAX_BUILDS_PER_ROUND = 6
# Kanäle: Pfad, baubar, Turmtyp one-hot (T), Upgrade-Stufe Pfad 1/2
N_CHANNELS = 2 + T + 2
N_SCALARS = 4


class BTDEnv(gym.Env):
    metadata = {"render_modes": ["ansi"]}

    def __init__(self, render_mode=None, reward_round=1.0, reward_life=-0.02,
                 reward_win=10.0, reward_loss=-5.0):
        self.game = Game()
        self.render_mode = render_mode
        self.rw = dict(round=reward_round, life=reward_life, win=reward_win, loss=reward_loss)
        self.action_space = spaces.Discrete(N_ACTIONS)
        self.observation_space = spaces.Box(
            0.0, 1.0, shape=(N_CHANNELS * N + N_SCALARS,), dtype=np.float32)
        self._static = np.stack([self.game.track.path_mask,
                                 self.game.track.buildable]).astype(np.float32)

    # ------------------------------------------------------------------
    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        self.game.reset()
        self.builds_left = MAX_BUILDS_PER_ROUND
        return self._obs(), self._info()

    def step(self, action: int):
        g, reward, info = self.game, 0.0, {}
        action = int(action)

        if action == 0 or self.builds_left <= 0:
            stats = g.play_round()
            info.update(stats)
            reward += self.rw["life"] * stats["leaked"]
            if g.lives > 0:
                reward += self.rw["round"]
            self.builds_left = MAX_BUILDS_PER_ROUND
        else:
            ok = self._do_build(action)
            info["invalid"] = not ok  # sollte durch die Maske nie passieren
            self.builds_left -= 1

        terminated = g.over
        if terminated:
            reward += self.rw["win"] if g.won else self.rw["loss"]
        return self._obs(), reward, terminated, False, {**self._info(), **info}

    def _do_build(self, a: int) -> bool:
        g = self.game
        if a < A_UPGRADE:
            k, c = divmod(a - A_PLACE, N)
            return g.place(k, c % GRID_W, c // GRID_W)
        if a < A_SELL:
            p, c = divmod(a - A_UPGRADE, N)
            return g.upgrade(c % GRID_W, c // GRID_W, p)
        c = a - A_SELL
        return g.sell(c % GRID_W, c // GRID_W)

    # ------------------------------------------------------------------
    def action_masks(self) -> np.ndarray:
        """Wird von MaskablePPO aufgerufen: True = erlaubte Aktion."""
        g = self.game
        mask = np.zeros(N_ACTIONS, dtype=bool)
        mask[0] = True
        if self.builds_left <= 0:
            return mask
        free = (g.track.buildable & ~g.occupied).ravel()
        for k, t in enumerate(TOWERS):
            if g.cash >= t.cost:
                mask[A_PLACE + k * N: A_PLACE + (k + 1) * N] = free
        for (x, y), tw in g.towers.items():
            c = y * GRID_W + x
            for p in (0, 1):
                mask[A_UPGRADE + p * N + c] = g.can_upgrade(x, y, p)
            # Verkaufen nur im Endspiel sinnvoll; erstmal erlaubt lassen
            mask[A_SELL + c] = True
        return mask

    def _obs(self) -> np.ndarray:
        g = self.game
        grid = np.zeros((N_CHANNELS, GRID_H, GRID_W), dtype=np.float32)
        grid[:2] = self._static
        for (x, y), tw in g.towers.items():
            grid[2 + tw.kind, y, x] = 1.0
            grid[2 + T, y, x] = tw.tiers[0] / MAX_TIER
            grid[3 + T, y, x] = tw.tiers[1] / MAX_TIER
        scalars = np.array([
            min(g.cash / 5000.0, 1.0),
            g.lives / Game.START_LIVES,
            g.round / engine.MAX_ROUND,
            self.builds_left / MAX_BUILDS_PER_ROUND,
        ], dtype=np.float32)
        return np.concatenate([grid.ravel(), scalars])

    def _info(self):
        g = self.game
        return {"round": g.round, "lives": g.lives, "cash": g.cash, "towers": len(g.towers)}

    def render(self):
        return self.game.render()
