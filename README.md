# bloons-ML

Reinforcement Learning (MaskablePPO) für Bloons TD 6 auf einer eigenen, headless Simulation.
Ziel-Modus: **CHIMPS** auf Monkey Meadow. Später soll die gelernte Policy über eine
Screen-Capture-Bridge im echten Spiel spielen.

## Stand

- `btd_sim/`: Python-Referenz-Engine (vereinfachte Bloons, 4 Türme, prozedurale Runden) + Gymnasium-Env mit Action-Masking
- `train.py` / `evaluate.py`: MaskablePPO-Training und Auswertung (bisher nur Pipeline-Test)
- `data/raw/towers/`: Turm-Daten aller BTD6-Affen aus dem Wiki (Rohformat, siehe `data/raw/towers/SCHEMA.md`)
- `data/base/towers/`: normalisierte Turm-Daten (erzeugt mit `tools/normalize.py`, Format: `data/schema/tower.schema.json`)
- `tools/wiki/`: Skripte, mit denen die Turm-Daten erzeugt wurden
- `docs/engine-design.md`: Design der geplanten Rust-Engine (`btd-core` + PyO3-Bindings)
- `mod/`: MelonLoader-Mod für BTD6, dumpt die Spieldaten (Türme inkl. aller Crosspaths, Bloons, Runden, Karten) nach `data/raw/game_dump/` (nicht im Repo), siehe `mod/README.md`

## Setup

```bash
uv venv .venv
uv pip install --python .venv/bin/python torch --index-url https://download.pytorch.org/whl/cpu
uv pip install --python .venv/bin/python -r requirements.txt
.venv/bin/python evaluate.py --random
```

Datenquelle: [bloonswiki.com](https://www.bloonswiki.com) (Stand der Spieldaten ≈ v55–56).
