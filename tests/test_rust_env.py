"""Rust-Env (btd_rs) gegen die Python-Referenz-Env (btd_sim.env).

Im f64-Build (`maturin develop ... --features f64`) müssen Observation, Reward, Masken und
Spielende Schritt für Schritt exakt übereinstimmen. Im f32-Build wird nur die Schnittstelle
geprüft, weil sich Positionen in f32 anders runden.
"""
import json

import numpy as np
import pytest

btd_rs = pytest.importorskip("btd_rs", reason="Rust-Bindings nicht gebaut (maturin develop)")

from btd_sim import BTDEnv  # noqa: E402
from btd_sim.rust_env import RustBTDEnv, RustVecEnv  # noqa: E402

EXACT = btd_rs.FLOAT == "f64"


def test_spaces_match():
    py, rs = BTDEnv(), RustBTDEnv()
    assert py.action_space == rs.action_space
    assert py.observation_space.shape == rs.observation_space.shape


@pytest.mark.skipif(not EXACT, reason="bit-genauer Vergleich nur im f64-Build")
@pytest.mark.parametrize("seed", range(6))
@pytest.mark.parametrize("masked", [True, False])
def test_step_by_step_parity(seed, masked):
    """masked=False: auch ungültige Aktionen und Aktionen am Bau-Limit (Masken nur vergleichen)."""
    rng = np.random.default_rng(seed)
    py, rs = BTDEnv(), RustBTDEnv()
    o1, _ = py.reset()
    o2, _ = rs.reset()
    steps = 0
    while True:
        m1, m2 = py.action_masks(), rs.action_masks()
        assert np.array_equal(o1, o2), f"Observation weicht ab (Schritt {steps})"
        assert np.array_equal(m1, m2), f"Maske weicht ab (Schritt {steps})"
        # Runde starten etwas seltener als bauen, damit Türme zusammenkommen
        valid = np.flatnonzero(m1) if masked else np.arange(len(m1))
        a = 0 if rng.random() < 0.25 else int(rng.choice(valid))
        o1, r1, d1, _, i1 = py.step(a)
        o2, r2, d2, _, i2 = rs.step(a)
        assert r1 == r2 and d1 == d2, f"Schritt {steps}: reward {r1} vs {r2}, done {d1} vs {d2}"
        assert (i1["round"], i1["lives"], i1["cash"]) == (i2["round"], i2["lives"], i2["cash"])
        steps += 1
        if d1:
            break
    assert np.array_equal(o1, o2)
    assert steps > 20


def test_replay_roundtrip(tmp_path):
    env = RustBTDEnv(record=True)
    rng = np.random.default_rng(1)
    env.reset()
    done = False
    while not done:
        a = int(rng.choice(np.flatnonzero(env.action_masks())))
        _, _, done, _, info = env.step(a)
    path = tmp_path / "game.jsonl"
    env.save_replay(str(path), last=True)
    lines = [json.loads(line) for line in path.read_text().splitlines()]
    assert lines[0]["type"] == "header" and lines[0]["format"] == btd_rs.REPLAY_FORMAT
    assert all(l["type"] == "round" for l in lines[1:])
    from btd_sim.rust_env import DEFAULT_DATA
    checks = btd_rs.replay_compare(DEFAULT_DATA, str(path))
    assert checks and all(c["matches"] for c in checks)

    # ab Runde 3 weiterspielen
    env2 = RustBTDEnv(record=True)
    env2.reset()
    obs = env2.load_replay(str(path), until_round=3)
    assert env2.env.round == 3 and obs.shape == env2.observation_space.shape


def test_vec_env_with_maskable_ppo():
    from sb3_contrib import MaskablePPO

    venv = RustVecEnv(4)
    model = MaskablePPO("MlpPolicy", venv, n_steps=64, batch_size=64, n_epochs=1, verbose=0)
    model.learn(total_timesteps=512)
    obs = venv.reset()
    masks = np.stack(venv.env_method("action_masks"))
    actions, _ = model.predict(obs, action_masks=masks, deterministic=True)
    assert all(masks[i, a] for i, a in enumerate(actions))


def test_vec_env_terminal_observation_and_episode_info():
    venv = RustVecEnv(2)
    obs = venv.reset()
    for _ in range(200):
        obs, rewards, dones, infos = venv.step(np.zeros(2, dtype=np.int64))  # nur Runden starten
        assert rewards.dtype == np.float32 and dones.dtype == bool
        if dones.any():
            i = int(np.flatnonzero(dones)[0])
            info = infos[i]
            assert info["terminal_observation"].shape == obs[i].shape
            assert not np.array_equal(info["terminal_observation"], obs[i]), "obs ist nach Auto-Reset neu"
            assert info["episode"]["l"] > 0 and info["round"] >= 0
            return
    pytest.fail("keine Episode beendet")


def test_strided_actions_are_accepted():
    venv = RustVecEnv(3)
    venv.reset()
    venv.step(np.zeros((3, 2), dtype=np.int64)[:, 0])  # nicht zusammenhängender View


def test_load_replay_errors():
    env = RustBTDEnv()
    with pytest.raises(OSError):
        env.load_replay("/gibt/es/nicht.jsonl")


def test_tower_bans_from_python():
    env = RustBTDEnv(max_banned_towers=2, seed=5)
    n_towers = len(env.env.banned)
    assert env.observation_space.shape[0] == BTDEnv().observation_space.shape[0] + n_towers
    obs, info = env.reset(options={"banned": [True] + [False] * (n_towers - 1)})
    assert info["banned"][0] and not any(info["banned"][1:])
    assert list(obs[-n_towers:]) == [0.0] + [1.0] * (n_towers - 1), "Werte 'verfügbar' am Ende"
    cells = (env.env.n_actions - 1) // (n_towers + 2 + 1)  # sim_v0: 4 Türme, 2 Pfade, Verkaufen
    assert cells == 300
    assert not env.action_masks()[1:1 + cells].any(), "gesperrter Turm ist nicht baubar"
    with pytest.raises(ValueError):
        env.reset(options={"banned": [True]})
