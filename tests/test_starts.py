"""Startzustände: Zustände aus Replays, Budgetkurve, Pool-Gewichte und die Anbindung an btd_rs."""
import json

import numpy as np
import pytest

pytest.importorskip("stable_baselines3")

from btd_sim.starts import StartPool, budget_curve, states_from_replay  # noqa: E402

DATA = {
    "rules": {"end_round": 5, "start_cash": 650, "cost_multiplier": 1.0},
    "towers": [{"name": "dart", "cost": 200, "upgrade_costs": [[100, 200], [50], []]}],
}


def replay(*rounds):
    lines = [json.dumps({"type": "header", "format": "btd-replay/2"})]
    for i, (actions, result) in enumerate(rounds, start=1):
        lines.append(json.dumps({"type": "round", "round": i, "actions": actions, "result": result}))
    return "\n".join(lines)


def test_states_from_replay_tracks_towers_until_each_round():
    text = replay(
        ([{"place": {"tower": "dart", "x": 1.0, "y": 2.0}}], {"lives": 150, "cash": 500, "round": 1}),
        ([{"upgrade": {"x": 1.0, "y": 2.0, "path": 0}},
          {"place": {"tower": "dart", "x": 5.0, "y": 5.0}}], {"lives": 140, "cash": 300, "round": 2}),
        ([{"sell": {"x": 1.0000001, "y": 2.0}}], {"lives": 0, "cash": 400, "round": 2}),  # verloren: kein Zustand
    )
    s = states_from_replay(text)
    assert [x["round"] for x in s] == [1, 2]
    assert s[0]["towers"] == [{"tower": "dart", "x": 1.0, "y": 2.0, "tiers": [0, 0, 0]}]
    assert [t["tiers"] for t in s[1]["towers"]] == [[1, 0, 0], [0, 0, 0]], "Bau-Reihenfolge, eigene Kopien"
    assert (s[1]["cash"], s[1]["lives"]) == (300.0, 140)
    assert [x["round"] for x in states_from_replay(text, end_round=2)] == [1], "nach der letzten Runde: nichts"


def test_budget_curve_interpolates_missing_rounds():
    states = [{"round": 2, "cash": 100.0, "lives": 1, "towers": [{"tower": "dart", "tiers": [1, 1, 0]}]}]
    curve = budget_curve(states, DATA)
    assert len(curve) == 6
    assert curve[0] == 650 and curve[2] == 100 + 200 + 100 + 50
    assert curve[1] == pytest.approx((650 + 450) / 2)
    assert curve[5] == curve[2], "nach dem letzten Wert konstant"
    chimps = dict(DATA, rules=dict(DATA["rules"], start_round=3))
    late = [dict(states[0], round=4)]
    c2 = budget_curve(late, chimps)
    assert c2[2] == 650 and c2[0] == 650, "Startgeld vor der ersten gespielten Runde, davor konstant"
    assert c2[3] == pytest.approx((650 + 450) / 2) and c2[4] == 450


def test_pool_weights_shares_quality_and_eviction():
    pool = StartPool(share_a=0.5, share_b=0.3, max_a=2, alpha=1.0, floor=0.1)
    b = [pool.add({"round": 1}, "B") for _ in range(3)]
    a = [pool.add({"round": 2}, "A") for _ in range(3)]
    states, weights, ids = pool.arrays()
    assert len(states) == 5 and a[0] not in ids, "ältester A-Zustand fliegt raus"
    w = dict(zip(ids, weights))
    assert sum(w[i] for i in b) == pytest.approx(0.3 / 0.8)
    assert sum(w[i] for i in a[1:]) == pytest.approx(0.5 / 0.8)
    pool.record(b[0], False)  # alpha 1: Qualität 0 -> Gewicht auf floor
    pool.record(a[0], True)  # nicht mehr im Pool: ignoriert
    _, weights, ids = pool.arrays()
    w2 = dict(zip(ids, weights))
    assert w2[b[0]] == pytest.approx(w[b[0]] * 0.1)


def test_vec_env_start_states_end_to_end():
    btd_rs = pytest.importorskip("btd_rs")
    from btd_sim.rust_env import DEFAULT_DATA, RustVecEnv
    if not hasattr(btd_rs.VecEnv, "set_starts"):
        pytest.skip("Rust-Bindings ohne Startzustände (maturin develop neu bauen)")
    env = RustVecEnv(4, DEFAULT_DATA, seed=1)
    single = btd_rs.Env(DEFAULT_DATA)
    single.reset()
    mask = single.action_masks()
    single.step(int(np.flatnonzero(mask)[1]))  # einen Turm bauen
    single.step(0)
    state = single.state()
    assert json.loads(state)["round"] == 1
    env.venv.set_starts([state], [1.0], 1.0, 0.0, [650.0] * 50, 0.3, [7])
    env.reset()
    assert list(env.venv.last_start_kinds) == [0, 0, 0, 0], "noch kein Schritt"
    for _ in range(3):
        env.step_async(np.zeros(4, dtype=np.int64))
        env.step_wait()
    assert all(k == 1 for k in env.venv.last_start_kinds)
    out = env.venv.take_outcomes()
    assert len(out) == 4 and all(o[0] == 1 and o[1] == 7 for o in out)
    with pytest.raises(ValueError):
        env.venv.set_starts(["kaputt"], [1.0], 1.0, 0.0, [], 0.3)
    e = btd_rs.Env(DEFAULT_DATA)
    e.reset_from(state)
    assert e.round == 1 and e.towers == 1
