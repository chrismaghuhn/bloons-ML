"""Vorausschau: Auswahl der Aktion (mit Stub-Env, ohne Modell)."""
from types import SimpleNamespace

import numpy as np
import pytest

pytest.importorskip("torch")
from btd_sim.lookahead import LookaheadPolicy  # noqa: E402


class StubEnv:
    """lookahead(action, rounds) aus einer Tabelle: action -> (lives_lost, cash)."""

    def __init__(self, table):
        self.table, self.calls = table, []

    def lookahead(self, action, rounds):
        self.calls.append((action, rounds))
        lost, cash = self.table[action]
        return {"lives_lost": lost, "cash": cash, "invalid": False, "rounds_played": rounds, "game_over": False}


# Probs kommen in den Tests direkt mit, das Modell wird nur in den Auswertungsmodus gesetzt
STUB_MODEL = SimpleNamespace(policy=SimpleNamespace(set_training_mode=lambda mode: None))


def choose(table, probs, mask=None, k=8):
    env = StubEnv(table)
    p = LookaheadPolicy(model=STUB_MODEL, env=env, k=k, horizon=2)
    mask = np.ones(len(probs), bool) if mask is None else np.asarray(mask)
    return p.choose(None, mask, probs=np.asarray(probs, float)), env, p


def test_starts_when_no_leak_predicted():
    a, env, _ = choose({None: (0, 100.0)}, [0.7, 0.2, 0.1])
    assert a == 0 and env.calls == [(None, 2)], "nur die eine Prüfung"


def test_non_start_actions_are_kept():
    a, env, _ = choose({}, [0.1, 0.8, 0.1])
    assert a == 1 and env.calls == []


def test_picks_fewest_lives_lost_then_more_cash():
    table = {None: (78, 0.0), 1: (10, 50.0), 2: (0, 10.0), 3: (0, 90.0), 4: (78, 999.0)}
    a, _, p = choose(table, [0.5, 0.2, 0.15, 0.1, 0.05])
    assert a == 3 and p.switched == 1, "0 Leben verloren, davon mehr Geld"


def test_keeps_start_when_nothing_is_better_and_respects_mask_and_k():
    table = {None: (5, 0.0), 1: (5, 999.0), 2: (0, 0.0)}
    a, _, _ = choose(table, [0.5, 0.3, 0.2], mask=[True, True, False])
    assert a == 0, "2 ist maskiert, 1 nicht besser"
    a, env, _ = choose(table, [0.5, 0.3, 0.2], k=1)
    assert a == 0 and (2, 2) not in env.calls, "nur die k wahrscheinlichsten Alternativen"
