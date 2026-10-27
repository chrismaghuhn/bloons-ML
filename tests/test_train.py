"""train.py: Entropie-Schedule."""
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

pytest.importorskip("stable_baselines3")
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from train import EntropySchedule  # noqa: E402


class Logger:
    def record(self, *a):
        pass


def test_entropy_schedule_is_linear_from_the_first_rollout():
    model = SimpleNamespace(ent_coef=0.01, num_timesteps=1000, logger=Logger())
    s = EntropySchedule(0.01, 0.001, total=1000)
    s.model = model
    seen = []
    for t in (1000, 1500, 2000, 3000):  # Lauf beginnt bei 1000 (z.B. --resume)
        model.num_timesteps = t
        s.on_rollout_start()
        seen.append(round(model.ent_coef, 6))
    assert seen == [0.01, 0.0055, 0.001, 0.001]
