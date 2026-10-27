"""Tests für die Python-Referenz-Sim (btd_sim)."""
import pytest

from btd_sim import engine
from btd_sim.engine import BLOON_ID, Game


@pytest.fixture
def single_round(monkeypatch):
    """Ersetzt die erste Runde durch 100 Rote im Abstand von 0.1 s."""
    rounds = list(engine.ROUNDS)
    rounds[0] = [(i * 0.1, BLOON_ID["red"]) for i in range(100)]
    monkeypatch.setattr(engine, "ROUNDS", rounds)


@pytest.mark.parametrize("cooldown", [0.12, 0.2, 0.23])
def test_cooldown_is_not_snapped_to_tick(single_round, cooldown):
    """Turm schneller als die Spawns: der 100. Schuss fällt nach 99 Cooldowns.
    Mit Einrasten aufs 0.05-s-Raster wären es 0.15/0.25/0.25 s pro Schuss."""
    g = Game()
    g.cash = 10_000
    assert g.place(0, 7, 4)
    tw = g.towers[(7, 4)]
    tw.range, tw.pierce, tw.cooldown = 99.0, 1, cooldown
    stats = g.play_round()
    assert stats["pops"] == 100 and stats["leaked"] == 0
    assert abs(stats["seconds"] - 99 * cooldown) <= 2 * engine.DT, stats


def test_idle_tower_does_not_bank_shots(monkeypatch):
    """Zwischen zwei Gruppen ohne Bloons darf der Turm keine Schüsse ansparen."""
    rounds = list(engine.ROUNDS)
    rounds[0] = [(0.0, BLOON_ID["red"])] * 10 + [(15.0, BLOON_ID["red"])] * 10
    monkeypatch.setattr(engine, "ROUNDS", rounds)
    g = Game()
    g.cash = 10_000
    g.place(0, 7, 4)
    tw = g.towers[(7, 4)]
    tw.range, tw.pierce, tw.cooldown = 99.0, 1, 1.0
    stats = g.play_round()
    assert stats["pops"] == 20
    # zweite Gruppe: Schüsse bei 15, 16, ..., 24 s. Mit angesparten Schüssen wäre sie nach ~15.5 s weg.
    assert stats["seconds"] >= 23.9, stats
