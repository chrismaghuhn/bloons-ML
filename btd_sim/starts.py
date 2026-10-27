"""Startzustände fürs Training (docs/design-start-states.md, Schritt 2).

- `states_from_replay`: Zustände zu Rundenbeginn aus einem Replay (Quelle B), ohne Nachsimulieren.
- `budget_curve`: Gesamtwert (Cash + Turmwert) je Runde aus Replays, interpoliert (für Quelle C).
- `StartPool`: Basiszustände der Quellen A und B mit Gewichtung nach 3-Runden-Überleben (gleitender
  Mittelwert), stabilen IDs und den Anteilen A/B.
- `StartStatesCallback`: sammelt Snapshots (A) und Ergebnisse, setzt den Pool neu und loggt
  Episoden-Kennzahlen getrennt nach Starttyp.
"""
from __future__ import annotations

import json
from collections import deque

import numpy as np
from stable_baselines3.common.callbacks import BaseCallback

KINDS = ("normal", "pool", "random")


def _tower_value(data: dict, tower: str, tiers) -> float:
    """Ausgegebenes Geld für einen Turm (Basispreis + gekaufte Upgrades), Medium-Preise × Faktor."""
    d = next(t for t in data["towers"] if t["name"] == tower)
    mult = data["rules"].get("cost_multiplier", 1.0)
    total = d["cost"] + sum(sum(d["upgrade_costs"][p][:tiers[p]]) for p in range(len(tiers)))
    return total * mult  # Rundung des Spiels ignoriert: nur für die Budgetkurve


def states_from_replay(text: str, end_round: int | None = None) -> list[dict]:
    """Ein Zustand je geschaffter Runde: Türme (Bau-Reihenfolge) aus den Aktionen bis Runde R,
    Cash und Leben aus der Ergebniszeile von Runde R. Runden ohne Ergebnis werden übersprungen,
    ebenso mit `end_round` der Stand nach der letzten Runde (danach gibt es nichts zu spielen)."""
    towers: list[dict] = []
    out = []

    def at(x, y):
        return min(range(len(towers)), key=lambda i: (towers[i]["x"] - x) ** 2 + (towers[i]["y"] - y) ** 2)

    for line in text.splitlines():
        if not line.strip():
            continue
        r = json.loads(line)
        if r.get("type") != "round":
            continue
        for a in r.get("actions", []):
            kind, v = next(iter(a.items()))
            if kind == "place":
                towers.append({"tower": v["tower"], "x": v["x"], "y": v["y"], "tiers": [0, 0, 0]})
            elif kind == "upgrade":
                towers[at(v["x"], v["y"])]["tiers"][v["path"]] += 1
            elif kind == "sell":
                towers.pop(at(v["x"], v["y"]))
        res = r.get("result") or {}
        if (res.get("lives", 0) > 0 and "cash" in res and "round" in res
                and (end_round is None or res["round"] < end_round)):
            out.append({"round": res["round"], "cash": float(res["cash"]), "lives": res["lives"],
                        "towers": [dict(t, tiers=list(t["tiers"])) for t in towers]})
    return out


def budget_curve(states: list[dict], data: dict) -> list[float]:
    """Median-Gesamtwert je Anzahl geschaffter Runden (0..Endrunde), Lücken linear interpoliert.
    Vor der ersten Runde (start_round - 1) zählt das Startgeld."""
    end = data["rules"]["end_round"]
    first = data["rules"].get("start_round", 1) - 1
    by_round: dict[int, list[float]] = {}
    for s in states:
        value = s["cash"] + sum(_tower_value(data, t["tower"], t["tiers"]) for t in s["towers"])
        by_round.setdefault(s["round"], []).append(value)
    by_round.setdefault(first, [float(data["rules"]["start_cash"])])
    xs = sorted(by_round)
    ys = [float(np.median(by_round[x])) for x in xs]
    return list(np.interp(np.arange(end + 1), xs, ys))


class StartPool:
    """Basiszustände mit stabilen IDs; Gewicht = Anteil der Quelle / Anzahl × Qualität.

    Qualität: gleitender Mittelwert (alpha) der 3-Runden-Überlebensquote, Start bei 1. Unter 50 %
    sinkt das Gewicht linear bis auf `floor` (hoffnungslose Zustände werden selten, nicht nie gezogen).
    Die Statistik gilt für den Basiszustand mit allen seinen (jedes Mal neu gewürfelten) Störungen.
    """

    def __init__(self, share_a=0.5, share_b=0.3, max_a=2000, alpha=0.2, floor=0.1):
        self.share = {"A": share_a, "B": share_b}
        self.max_a, self.alpha, self.floor = max_a, alpha, floor
        self.entries: dict[int, dict] = {}
        self.a_ids: deque[int] = deque()
        self._next = 0

    def add(self, state: dict | str, source: str) -> int:
        sid = self._next
        self._next += 1
        text = state if isinstance(state, str) else json.dumps(state)
        self.entries[sid] = {"state": text, "source": source, "quality": 1.0, "n": 0}
        if source == "A":
            self.a_ids.append(sid)
            while len(self.a_ids) > self.max_a:  # älteste A-Zustände fliegen raus
                self.entries.pop(self.a_ids.popleft(), None)
        return sid

    def record(self, sid: int, survived: bool):
        e = self.entries.get(sid)
        if e is not None:  # aus dem Pool gefallene Zustände ignorieren
            e["quality"] += self.alpha * (float(survived) - e["quality"])
            e["n"] += 1

    def arrays(self):
        """(Zustände als JSON, Gewichte, IDs) für `VecEnv.set_starts`."""
        ids = list(self.entries)
        count = {s: sum(1 for i in ids if self.entries[i]["source"] == s) for s in self.share}
        present = {s: self.share[s] for s in self.share if count[s]}
        norm = sum(present.values()) or 1.0
        weights = []
        for i in ids:
            e = self.entries[i]
            q = 1.0 if e["quality"] >= 0.5 else max(self.floor, e["quality"] / 0.5)
            weights.append(present[e["source"]] / norm / count[e["source"]] * q)
        return [self.entries[i]["state"] for i in ids], weights, ids


class StartStatesCallback(BaseCallback):
    """Pool pflegen und Kennzahlen je Starttyp loggen (`RustVecEnv` mit Startzuständen)."""

    def __init__(self, pool: StartPool, budget: list[float], p_start=0.3, p_random=0.2, every=1, valid=None):
        """`valid(state_json) -> bool` prüft eigene Snapshots vor der Aufnahme (ein ungültiger Zustand
        würde sonst `set_starts` und damit das Training abbrechen)."""
        super().__init__()
        self.valid = valid
        self.rejected = 0
        self.pool, self.budget = pool, budget
        self.p_start, self.p_random, self.every = p_start, p_random, every
        self.steps = np.zeros(3)
        self.episodes = {k: deque(maxlen=100) for k in KINDS}
        self.checks = {"alle": deque(maxlen=500), "leben_gestört": deque(maxlen=500)}
        self._iter = 0
        self.fallbacks = 0

    def _push(self):
        states, weights, ids = self.pool.arrays()
        # Anteil zufälliger Builds bezogen auf alle Starts aus Zuständen (Quelle C)
        self.training_env.venv.set_starts(states, weights, self.p_start, self.p_random, self.budget, 0.3, ids)

    def _on_training_start(self):
        self._push()

    def _on_step(self):
        venv = self.training_env.venv
        kinds = venv.last_start_kinds
        self.steps += np.bincount(kinds, minlength=3)[:3]
        for i, info in enumerate(self.locals["infos"]):
            if "episode" in info:
                ep = info["episode"]
                self.episodes[KINDS[kinds[i]]].append((ep["r"], info.get("round", 0), ep["l"]))
        return True

    def _on_rollout_end(self):
        venv = self.training_env.venv
        for kind, sid, survived, lives_perturbed in venv.take_outcomes():
            if sid >= 0:
                self.pool.record(sid, survived)
            self.checks["alle"].append(survived)
            if lives_perturbed:
                self.checks["leben_gestört"].append(survived)
        for s in venv.take_snapshots():
            if self.valid is None or self.valid(s):
                self.pool.add(s, "A")
            else:
                self.rejected += 1
        self.logger.record("starts/snapshots_verworfen", self.rejected)
        total = self.steps.sum() or 1.0
        for k, name in enumerate(KINDS):
            self.logger.record(f"starts/schritte_{name}", self.steps[k] / total)
            if self.episodes[name]:
                r, rnd, length = zip(*self.episodes[name])
                self.logger.record(f"starts/{name}_return", float(np.mean(r)))
                self.logger.record(f"starts/{name}_round", float(np.mean(rnd)))
                self.logger.record(f"starts/{name}_length", float(np.mean(length)))
        for name, c in self.checks.items():
            if c:
                self.logger.record(f"starts/sofort_verloren_{name}", 1.0 - float(np.mean(c)))
        self.fallbacks += venv.take_fallbacks()
        self.logger.record("starts/fehlgeschlagen", self.fallbacks)  # Starts, die normal liefen
        self.logger.record("starts/pool_a", len(self.pool.a_ids))
        self.logger.record("starts/pool", len(self.pool.entries))
        self._iter += 1
        if self._iter % self.every == 0:
            self._push()
