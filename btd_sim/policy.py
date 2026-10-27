"""Faltungs-Policy für den Raster-Aktionsraum der Rust-Env.

Die Observation ist ein Raster (Kanäle × H × W: Pfad, baubar, Turmtyp, Tiers) plus Skalare
(Cash, Leben, Runde, Bau-Aktionen, Runden-Vorschau). Fast alle Aktionen gehören zu einer Zelle
(Turm k bauen, Pfad p upgraden, verkaufen), deshalb gibt das Netz pro Zelle direkt Logits aus
(1×1-Faltung), statt sie aus einem flachen Vektor zu raten. Die Skalare gehen als konstante
Ebenen ins Raster, "Runde starten" und der Wert kommen aus gepoolten Merkmalen.

Aktions-Layout wie `Layout` in engine/crates/btd-env/src/lib.rs:
`0` Runde starten, dann je Aktionsart (T Türme, P Pfade, Verkaufen) ein Block über alle Zellen.
"""
from __future__ import annotations

import json

import torch
from sb3_contrib.common.maskable.policies import MaskableActorCriticPolicy
from torch import nn


def grid_spec(data_path: str, obs_size: int, n_actions: int) -> dict:
    """Raster-Maße aus der game_v1-Datei, geprüft gegen Observation und Aktionsraum der Env."""
    with open(data_path) as f:
        d = json.load(f)
    g = d["map"]["grid"]
    towers = len(d["towers"])
    paths = max(max((p + 1 for p, c in enumerate(t["upgrade_costs"]) if c), default=0) for t in d["towers"])
    h, w = g["h"], g["w"]
    channels = 2 + towers + paths
    scalars = obs_size - channels * h * w
    heads = towers + paths + 1
    if scalars < 0 or 1 + heads * h * w != n_actions:
        raise ValueError(f"Layout passt nicht: obs {obs_size}, Aktionen {n_actions}, Raster {channels}×{h}×{w}")
    return dict(channels=channels, height=h, width=w, scalars=scalars, heads=heads)


class ResBlock(nn.Module):
    def __init__(self, ch: int, dilation: int):
        super().__init__()
        self.c1 = nn.Conv2d(ch, ch, 3, padding=dilation, dilation=dilation)
        self.c2 = nn.Conv2d(ch, ch, 3, padding=1)
        self.act = nn.ReLU()

    def forward(self, x):
        return self.act(x + self.c2(self.act(self.c1(x))))


class GridNet(nn.Module):
    """Gemeinsamer Rumpf für Policy und Wert (sb3: `mlp_extractor`)."""

    def __init__(self, channels, height, width, scalars, heads, width_ch=32, blocks=4, scalar_dim=32):
        super().__init__()
        self.c, self.h, self.w, self.s = channels, height, width, scalars
        self.scalar_net = nn.Sequential(nn.Linear(max(scalars, 1), scalar_dim), nn.ReLU())
        self.stem = nn.Sequential(nn.Conv2d(channels + scalar_dim + 2, width_ch, 3, padding=1), nn.ReLU())
        dilations = [1, 2, 4] * (blocks // 3) + [1] * (blocks % 3)
        self.trunk = nn.Sequential(*[ResBlock(width_ch, d) for d in dilations])
        self.cell_head = nn.Conv2d(width_ch, heads, 1)
        pooled = 2 * width_ch + scalar_dim
        self.global_pi = nn.Sequential(nn.Linear(pooled, 128), nn.ReLU(), nn.Linear(128, 1))
        # Logits kommen fertig aus forward_actor; sb3 legt in _build trotzdem ein
        # Linear(latent_dim_pi, n_actions) an, das GridPolicy gleich verwirft. Mit 1 bleibt es winzig
        # (sonst n_actions² Gewichte, ~200 MB bei 7201 Aktionen).
        self.latent_dim_pi = 1
        self.latent_dim_vf = 256
        self.vf = nn.Sequential(nn.Linear(pooled, 256), nn.ReLU())
        # Koordinaten-Ebenen, damit das Netz weiß, wo auf der Karte eine Zelle liegt
        ys = torch.linspace(-1, 1, height).view(1, 1, height, 1).expand(1, 1, height, width)
        xs = torch.linspace(-1, 1, width).view(1, 1, 1, width).expand(1, 1, height, width)
        self.register_buffer("coords", torch.cat([xs, ys], 1), persistent=False)

    def _features(self, obs):
        n = obs.shape[0]
        cells = self.h * self.w
        grid = obs[:, : self.c * cells].view(n, self.c, self.h, self.w)
        sc = obs[:, self.c * cells :] if self.s > 0 else obs.new_zeros(n, 1)
        s = self.scalar_net(sc)
        planes = s.view(n, -1, 1, 1).expand(-1, -1, self.h, self.w)
        x = torch.cat([grid, planes, self.coords.expand(n, -1, -1, -1)], 1)
        x = self.trunk(self.stem(x))
        pooled = torch.cat([x.mean((2, 3)), x.amax((2, 3)), s], 1)
        return x, pooled

    def forward(self, obs):
        x, pooled = self._features(obs)  # Rumpf nur einmal für beide Köpfe
        return self._logits(x, pooled), self.vf(pooled)

    def _logits(self, x, pooled):
        cells = self.cell_head(x).flatten(1)  # (n, heads*H*W): Block je Aktionsart, Zellen zeilenweise
        return torch.cat([self.global_pi(pooled), cells], 1)

    def forward_actor(self, obs):
        return self._logits(*self._features(obs))

    def forward_critic(self, obs):
        _, pooled = self._features(obs)
        return self.vf(pooled)


class GridPolicy(MaskableActorCriticPolicy):
    """MaskablePPO-Policy mit `GridNet`; die Logits kommen direkt aus dem Netz (kein action_net)."""

    def __init__(self, *args, grid: dict, width_ch: int = 32, blocks: int = 4, **kwargs):
        self.grid = grid
        self.width_ch, self.blocks = width_ch, blocks
        kwargs.setdefault("share_features_extractor", True)
        super().__init__(*args, **kwargs)

    def _build_mlp_extractor(self) -> None:
        self.mlp_extractor = GridNet(**self.grid, width_ch=self.width_ch, blocks=self.blocks)

    def _build(self, lr_schedule) -> None:
        super()._build(lr_schedule)
        self.action_net = nn.Identity()
        if self.ortho_init:
            # wie sb3 für action_net: kleine Gewichte, damit die Start-Policy fast gleichverteilt ist
            for m in (self.mlp_extractor.cell_head, self.mlp_extractor.global_pi[-1]):
                nn.init.orthogonal_(m.weight, gain=0.01)
                nn.init.zeros_(m.bias)
        self.optimizer = self.optimizer_class(self.parameters(), lr=lr_schedule(1), **self.optimizer_kwargs)

    def _get_constructor_parameters(self) -> dict:
        data = super()._get_constructor_parameters()
        data.update(grid=self.grid, width_ch=self.width_ch, blocks=self.blocks)
        return data
