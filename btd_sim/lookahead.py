"""Vorausschau beim Spielen (docs/design-lookahead.md).

Will die Policy die Runde starten, spielt die Sim auf einer Kopie die nächsten `horizon` Runden durch.
Leakt dabei etwas, werden die `k` wahrscheinlichsten anderen erlaubten Aktionen der Policy einzeln
ausprobiert (Aktion, dann `horizon` Runden) und die mit den wenigsten verlorenen Leben genommen; bei
Gleichstand die mit mehr übrigem Geld, dann die wahrscheinlichere. Ist keine besser als "starten",
wird gestartet. Nur fürs Spielen, das Training bleibt unverändert.
"""
from __future__ import annotations

import numpy as np
import torch


def action_probs(model, obs, mask) -> np.ndarray:
    """Wahrscheinlichkeiten der Policy über alle Aktionen (maskierte = 0)."""
    obs_t, _ = model.policy.obs_to_tensor(obs)
    with torch.no_grad():
        dist = model.policy.get_distribution(obs_t, action_masks=np.asarray(mask)[None])
        return dist.distribution.probs[0].float().cpu().numpy()


class LookaheadPolicy:
    """Wie `model.predict`, mit Vorausschau vor jedem Rundenstart. `env` ist die `btd_rs.Env`, auf der
    gespielt wird (z.B. `RustBTDEnv.env`)."""

    def __init__(self, model, env, k: int = 8, horizon: int = 2, deterministic: bool = True):
        self.model, self.env, self.k, self.horizon = model, env, k, horizon
        self.deterministic = deterministic
        self.switched = 0  # wie oft die Vorausschau den Rundenstart ersetzt hat

    def choose(self, obs, mask, probs=None) -> int:
        probs = action_probs(self.model, obs, mask) if probs is None else probs
        if self.deterministic:
            a = int(np.argmax(probs))
        else:
            a = int(np.random.choice(len(probs), p=probs / probs.sum()))
        if a != 0:
            return a
        base = self.env.lookahead(None, self.horizon)
        if base["lives_lost"] <= 0:
            return 0
        cands = [int(c) for c in np.argsort(-probs) if c != 0 and mask[c] and probs[c] > 0][: self.k]
        best, best_key = 0, None
        for c in cands:
            r = self.env.lookahead(c, self.horizon)
            if r["invalid"]:
                continue
            key = (r["lives_lost"], -r["cash"], -probs[c])
            if r["lives_lost"] < base["lives_lost"] and (best_key is None or key < best_key):
                best, best_key = c, key
        if best:
            self.switched += 1
        return best

    def predict(self, obs, action_masks):
        return self.choose(obs, action_masks), None
