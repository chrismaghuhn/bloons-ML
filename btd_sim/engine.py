"""Vereinfachte, schnelle BTD6-ähnliche Simulation (headless).

Karte: Monkey Meadow, abgebildet auf ein 20x15-Raster.
Eine Zelle = 100 px bei einem 2000x1500-Screenshot, d.h. Zelle (cx, cy)
entspricht im echten Spiel dem Pixel (cx*100+50, cy*100+50).
"""
from __future__ import annotations

from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path
import json

import numpy as np

# ---------------------------------------------------------------- Karte
GRID_W, GRID_H = 20, 15
CELL_PX = 100
DT = 0.05  # Sekunden pro Simulationsschritt
CD_EPS = 1e-9  # Toleranz für Float-Fehler beim Cooldown-Vergleich

# Pfad-Mittellinie in Zellkoordinaten (Zellmittelpunkte), aus dem Screenshot abgelesen
MONKEY_MEADOW = [
    (-1, 5), (10, 5),   # von links rein, nach rechts
    (10, 2), (6, 2),    # hoch, nach links (obere Schleife)
    (6, 10), (3, 10),   # runter (kreuzt sich selbst), nach links
    (3, 7), (12, 7),    # hoch, lange Gerade nach rechts
    (12, 4), (15, 4),   # hoch, rechts
    (15, 9), (9, 9),    # runter, nach links
    (9, 15),            # runter, raus
]
UI_BLOCKED_ROWS = range(13, GRID_H)  # Tower-Leiste unten im echten Spiel


class Track:
    def __init__(self, waypoints):
        pts = np.array([(x + 0.5, y + 0.5) for x, y in waypoints], dtype=np.float64)
        seg = np.diff(pts, axis=0)
        seg_len = np.hypot(seg[:, 0], seg[:, 1])
        self.pts = pts
        self.cum = np.concatenate([[0.0], np.cumsum(seg_len)])
        self.length = float(self.cum[-1])

        # Pfadzellen rastern
        self.path_mask = np.zeros((GRID_H, GRID_W), dtype=bool)
        for d in np.arange(0, self.length, 0.1):
            x, y = self.xy(np.array([d]))
            cx, cy = int(x[0]), int(y[0])
            if 0 <= cx < GRID_W and 0 <= cy < GRID_H:
                self.path_mask[cy, cx] = True
        self.buildable = ~self.path_mask
        for r in UI_BLOCKED_ROWS:
            self.buildable[r, :] = False

    def xy(self, dist: np.ndarray):
        return (np.interp(dist, self.cum, self.pts[:, 0]),
                np.interp(dist, self.cum, self.pts[:, 1]))


# ---------------------------------------------------------------- Bloons
# name: (Kinder, Geschwindigkeit relativ zu Rot, RBE, Immunitäten)
BLOONS = {
    "red":    ((),                 1.0, 1,  ()),
    "blue":   (("red",),           1.4, 2,  ()),
    "green":  (("blue",),          1.8, 3,  ()),
    "yellow": (("green",),         3.2, 4,  ()),
    "pink":   (("yellow",),        3.5, 5,  ()),
    "black":  (("pink", "pink"),   1.8, 11, ("explosive",)),
    "white":  (("pink", "pink"),   2.0, 11, ()),
    "lead":   (("black", "black"), 1.0, 23, ("sharp",)),
    "zebra":  (("black", "white"), 1.8, 23, ("explosive",)),
}
BLOON_NAMES = list(BLOONS)
BLOON_ID = {n: i for i, n in enumerate(BLOON_NAMES)}
BASE_SPEED = 2.5  # Zellen pro Sekunde für Rot
SPEED = np.array([BLOONS[n][1] * BASE_SPEED for n in BLOON_NAMES])
RBE = np.array([BLOONS[n][2] for n in BLOON_NAMES])


@lru_cache(maxsize=None)
def pop(bloon: int, damage: int, dmg_type: str) -> tuple[tuple[int, ...], int]:
    """Schaden auf einen Bloon anwenden -> (übrige Bloons, Anzahl Pops = Geld)."""
    name = BLOON_NAMES[bloon]
    children, _, _, immune = BLOONS[name]
    if damage <= 0 or dmg_type in immune:
        return (bloon,), 0
    out, pops = [], 1
    for child in children:  # Überschuss-Schaden geht an die Kinder weiter
        c, p = pop(BLOON_ID[child], damage - 1, dmg_type)
        out.extend(c)
        pops += p
    return tuple(out), pops


# ---------------------------------------------------------------- Türme
@dataclass(frozen=True)
class TowerType:
    name: str
    cost: int
    range: float
    cooldown: float
    damage: int
    pierce: int
    dmg_type: str            # "sharp" | "explosive" | "normal"
    blast: float = 0.0       # > 0: Flächenschaden um das Ziel
    upgrades: tuple = ((), ())  # 2 Pfade x 3 Stufen: (Kosten, Effekte)


TOWERS = [
    TowerType("dart", 200, 3.2, 0.95, 1, 2, "sharp", upgrades=(
        ((90, {"pierce": 1}), (120, {"pierce": 2}), (500, {"damage": 1, "dmg_type": "normal"})),
        ((100, {"range": 0.8}), (190, {"rate": 0.8}), (400, {"rate": 0.6})),
    )),
    TowerType("tack", 280, 2.3, 1.4, 1, 8, "sharp", upgrades=(
        ((150, {"rate": 0.8}), (300, {"rate": 0.75}), (600, {"damage": 1})),
        ((100, {"range": 0.5}), (250, {"pierce": 4}), (500, {"pierce": 8})),
    )),
    TowerType("sniper", 350, 99.0, 1.6, 2, 1, "normal", upgrades=(
        ((350, {"damage": 2}), (1100, {"damage": 3}), (3000, {"damage": 8})),
        ((250, {"rate": 0.7}), (450, {"rate": 0.6}), (1800, {"pierce": 2})),
    )),
    TowerType("bomb", 525, 3.3, 1.5, 1, 14, "explosive", blast=1.2, upgrades=(
        ((250, {"blast": 0.4, "pierce": 8}), (650, {"damage": 1}), (1200, {"damage": 2, "blast": 0.5})),
        ((250, {"rate": 0.75}), (400, {"range": 0.7}), (900, {"rate": 0.6})),
    )),
]
MAX_TIER = 3
MAX_SECOND_PATH = 2  # wie in BTD6: nur ein Pfad darf auf die höchste Stufe
SELL_RATIO = 0.7
CHILD_SPACING = 0.15  # Kinder spawnen leicht hinter dem Parent (Zellen)
ROUND_BONUS_BASE = 100  # Rundenbonus = ROUND_BONUS_BASE + Rundennummer


@dataclass
class Tower:
    kind: int
    x: int
    y: int
    tiers: list = field(default_factory=lambda: [0, 0])
    spent: int = 0
    cd_left: float = 0.0
    # aktuelle Werte (werden durch Upgrades verändert)
    range: float = 0.0
    cooldown: float = 0.0
    damage: int = 0
    pierce: int = 0
    dmg_type: str = ""
    blast: float = 0.0

    @classmethod
    def build(cls, kind: int, x: int, y: int) -> "Tower":
        t = TOWERS[kind]
        return cls(kind, x, y, spent=t.cost, range=t.range, cooldown=t.cooldown,
                   damage=t.damage, pierce=t.pierce, dmg_type=t.dmg_type, blast=t.blast)

    def next_upgrade(self, path: int):
        tier = self.tiers[path]
        if tier >= MAX_TIER:
            return None
        if tier + 1 > MAX_SECOND_PATH and self.tiers[1 - path] > MAX_SECOND_PATH:
            return None  # nur ein Pfad darf über MAX_SECOND_PATH
        return TOWERS[self.kind].upgrades[path][tier]

    def apply(self, path: int, cost: int, fx: dict):
        self.tiers[path] += 1
        self.spent += cost
        self.pierce += fx.get("pierce", 0)
        self.damage += fx.get("damage", 0)
        self.range += fx.get("range", 0.0)
        self.blast += fx.get("blast", 0.0)
        self.cooldown *= fx.get("rate", 1.0)
        self.dmg_type = fx.get("dmg_type", self.dmg_type)


# ---------------------------------------------------------------- Runden
MAX_ROUND = 40
_UNLOCK = [("red", 1), ("blue", 3), ("green", 6), ("yellow", 10), ("pink", 14),
           ("black", 19), ("white", 22), ("lead", 25), ("zebra", 30)]


def _make_rounds(seed: int = 1234):
    """Feste, prozedurale Runden (wie in BTD6 ist jede Runde immer gleich)."""
    rng = np.random.default_rng(seed)
    rounds = []
    for r in range(1, MAX_ROUND + 1):
        avail = [n for n, start in _UNLOCK if r >= start]
        budget = 12 + 3.2 * r + 0.18 * r * r  # Ziel-RBE der Runde
        spawns, t = [], 0.0
        while budget > 0:
            # neuere Typen sind wahrscheinlicher
            w = np.arange(1, len(avail) + 1, dtype=float) ** 1.5
            name = avail[rng.choice(len(avail), p=w / w.sum())]
            n = int(rng.integers(3, 10))
            gap = max(0.12, 0.8 - 0.015 * r) * rng.uniform(0.7, 1.3)
            for _ in range(n):
                spawns.append((t, BLOON_ID[name]))
                t += gap
                budget -= RBE[BLOON_ID[name]]
                if budget <= 0:
                    break
            t += rng.uniform(0.5, 2.0)
        rounds.append(spawns)
    return rounds


def _load_rounds():
    """rounds.json (z.B. aus dem Wiki) hat Vorrang, sonst prozedurale Runden.

    Format: [[[zeit_s, "bloon_name"], ...], ...]  -- eine Liste pro Runde.
    """
    global MAX_ROUND
    path = Path(__file__).with_name("rounds.json")
    if not path.exists():
        return _make_rounds()
    data = json.loads(path.read_text())
    MAX_ROUND = len(data)
    return [[(float(t), BLOON_ID[name]) for t, name in r] for r in data]


ROUNDS = _load_rounds()


# ---------------------------------------------------------------- Spiel
class Game:
    START_CASH = 650
    START_LIVES = 150
    MAX_ROUND_SECONDS = 300

    def __init__(self, waypoints=MONKEY_MEADOW):
        self.track = Track(waypoints)
        self.reset()

    def reset(self):
        self.cash = self.START_CASH
        self.lives = self.START_LIVES
        self.round = 0  # Anzahl geschaffter Runden
        self.towers: dict[tuple[int, int], Tower] = {}
        self.occupied = np.zeros((GRID_H, GRID_W), dtype=bool)

    # -- Aktionen ------------------------------------------------------
    @property
    def over(self) -> bool:
        return self.lives <= 0 or self.round >= MAX_ROUND

    @property
    def won(self) -> bool:
        return self.lives > 0 and self.round >= MAX_ROUND

    def can_place(self, kind: int, x: int, y: int) -> bool:
        return (self.track.buildable[y, x] and not self.occupied[y, x]
                and self.cash >= TOWERS[kind].cost)

    def place(self, kind: int, x: int, y: int) -> bool:
        if not self.can_place(kind, x, y):
            return False
        self.cash -= TOWERS[kind].cost
        self.towers[(x, y)] = Tower.build(kind, x, y)
        self.occupied[y, x] = True
        return True

    def can_upgrade(self, x: int, y: int, path: int) -> bool:
        t = self.towers.get((x, y))
        if t is None:
            return False
        up = t.next_upgrade(path)
        return up is not None and self.cash >= up[0]

    def upgrade(self, x: int, y: int, path: int) -> bool:
        if not self.can_upgrade(x, y, path):
            return False
        t = self.towers[(x, y)]
        cost, fx = t.next_upgrade(path)
        self.cash -= cost
        t.apply(path, cost, fx)
        return True

    def sell(self, x: int, y: int) -> bool:
        t = self.towers.pop((x, y), None)
        if t is None:
            return False
        self.cash += int(t.spent * SELL_RATIO)
        self.occupied[y, x] = False
        return True

    # -- Runde simulieren ---------------------------------------------
    def play_round(self) -> dict:
        """Spielt die nächste Runde komplett durch."""
        spawns = ROUNDS[self.round]
        si = 0
        dist = np.empty(0)
        kind = np.empty(0, dtype=np.int64)
        lives_before, pops_total, t = self.lives, 0, 0.0
        towers = list(self.towers.values())
        for tw in towers:
            tw.cd_left = 0.0
        track = self.track

        while t < self.MAX_ROUND_SECONDS:
            # Spawnen
            new = []
            while si < len(spawns) and spawns[si][0] <= t:
                new.append(spawns[si][1])
                si += 1
            if new:
                dist = np.concatenate([dist, np.zeros(len(new))])
                kind = np.concatenate([kind, np.array(new, dtype=np.int64)])

            # Bewegen + Durchgelassene
            if len(dist):
                dist = dist + SPEED[kind] * DT
                leaked = dist >= track.length
                if leaked.any():
                    self.lives -= int(RBE[kind[leaked]].sum())
                    dist, kind = dist[~leaked], kind[~leaked]
                    if self.lives <= 0:
                        self.lives = 0
                        break

            # Türme schießen. Cooldowns laufen jeden Tick (auch ohne Bloons) und der Überschuss
            # wird übertragen (cd_left += cooldown), sonst rasten sie auf das Tick-Raster ein.
            for tw in towers:
                tw.cd_left -= DT
            if len(dist):
                bx, by = track.xy(dist)
                for tw in towers:
                    if tw.cd_left > CD_EPS or not len(dist):
                        continue
                    cx, cy = tw.x + 0.5, tw.y + 0.5
                    in_range = np.flatnonzero((bx - cx) ** 2 + (by - cy) ** 2 <= tw.range ** 2)
                    if not len(in_range):
                        tw.cd_left = 0.0  # schussbereit, aber kein Ziel: nicht weiter "ansparen"
                        continue
                    first = in_range[np.argmax(dist[in_range])]  # Targeting: "First"
                    if tw.blast > 0:
                        hit = np.flatnonzero((bx - bx[first]) ** 2 + (by - by[first]) ** 2 <= tw.blast ** 2)
                    else:
                        hit = in_range
                    hit = hit[np.argsort(-dist[hit], kind="stable")][: tw.pierce]  # stabil: gleiche Distanz -> Index-Reihenfolge
                    tw.cd_left = max(tw.cd_left, -DT) + tw.cooldown

                    keep = np.ones(len(dist), dtype=bool)
                    add_d, add_k = [], []
                    for i in hit:
                        rest, pops = pop(int(kind[i]), tw.damage, tw.dmg_type)
                        if pops == 0:
                            continue
                        keep[i] = False
                        pops_total += pops
                        for j, c in enumerate(rest):  # Kinder leicht versetzt
                            add_d.append(dist[i] - CHILD_SPACING * j)
                            add_k.append(c)
                    if not keep.all() or add_d:
                        dist = np.concatenate([dist[keep], np.array(add_d)])
                        kind = np.concatenate([kind[keep], np.array(add_k, dtype=np.int64)])
                        bx, by = track.xy(dist)

            t += DT
            if si >= len(spawns) and not len(dist):
                break

        self.cash += pops_total
        if self.lives > 0:
            self.round += 1
            self.cash += ROUND_BONUS_BASE + self.round
        return {"leaked": lives_before - self.lives, "pops": pops_total, "seconds": t}

    # -- Debug-Ausgabe -------------------------------------------------
    def render(self) -> str:
        sym = {i: t.name[0].upper() for i, t in enumerate(TOWERS)}
        rows = []
        for y in range(GRID_H):
            row = ""
            for x in range(GRID_W):
                if (x, y) in self.towers:
                    row += sym[self.towers[(x, y)].kind]
                elif self.track.path_mask[y, x]:
                    row += "#"
                elif not self.track.buildable[y, x]:
                    row += "~"
                else:
                    row += "."
            rows.append(row)
        rows.append(f"Runde {self.round}/{MAX_ROUND}  Leben {self.lives}  ${self.cash}")
        return "\n".join(rows)
