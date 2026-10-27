# bloons-ML

Reinforcement Learning (MaskablePPO) für Bloons TD 6 auf einer eigenen, headless Simulation.
Ziel-Modus: **CHIMPS** auf Monkey Meadow. Später soll die gelernte Policy über eine
Screen-Capture-Bridge im echten Spiel spielen.

## Stand

- `engine/`: Rust-Engine (`btd-data`, `btd-core`, `btd-env`, `btd-tools`, `btd-py`), im f64-Build bit-genau zur Python-Referenz
- `btd_sim/`: Python-Referenz-Engine (vereinfachte Bloons, 4 Türme, prozedurale Runden) + Gymnasium-Env mit Action-Masking
- `train.py` / `evaluate.py`: MaskablePPO-Training und Auswertung (bisher nur Pipeline-Test)
- `data/raw/towers/`: Turm-Daten aller BTD6-Affen aus dem Wiki (Rohformat, siehe `data/raw/towers/SCHEMA.md`)
- `data/base/towers/`: normalisierte Turm-Daten (erzeugt mit `tools/normalize.py`, Format: `data/schema/tower.schema.json`)
- `tools/wiki/`: Skripte, mit denen die Turm-Daten erzeugt wurden
- `docs/engine-design.md`: Design der geplanten Rust-Engine (`btd-core` + PyO3-Bindings)

## Rust-Engine

```bash
# Python-Bindings bauen (schnell, f32); mit --features f64 bit-genau zur Python-Sim
.venv/bin/maturin develop --release -m engine/crates/btd-py/Cargo.toml
# Training auf der Rust-Engine (Standard) bzw. der Python-Referenz
.venv/bin/python train.py --steps 400000 --envs 64
.venv/bin/python train.py --engine python --envs 12
# Rust-Tests, Paritätstest gegen die Python-Sim, Benchmark
cd engine && cargo test && cargo test -p btd-core --features f64
cargo run --release -p btd-tools -- bench --envs 64
```

## Spielprotokolle (Replays)

Mit `RustBTDEnv(record=True)` wird jedes Spiel als `.jsonl` mitgeschrieben (`save_replay`).
Ein Protokoll lässt sich nachspielen, gegen die Sim vergleichen oder ab einer Runde fortsetzen:

```bash
cargo run --release -p btd-tools -- replay spiel.jsonl            # strikt nachspielen
cargo run --release -p btd-tools -- replay spiel.jsonl --compare  # Abweichungen auflisten
```

Ein echtes Spiel kann man von Hand aufschreiben, `result` darf dabei unvollständig sein:

```json
{"type":"header","format":"btd-replay/1","data":"sim_v0.json"}
{"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7,"y":4}}],"result":{"lives":150}}
{"type":"round","round":2,"actions":[{"upgrade":{"x":7,"y":4,"path":0}}],"result":{"lives":150,"cash":512}}
```

## Setup

```bash
uv venv .venv
uv pip install --python .venv/bin/python torch --index-url https://download.pytorch.org/whl/cpu
uv pip install --python .venv/bin/python -r requirements.txt
.venv/bin/python evaluate.py --random
```

Datenquelle: [bloonswiki.com](https://www.bloonswiki.com) (Stand der Spieldaten ≈ v55–56).
