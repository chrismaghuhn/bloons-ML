# bloons-ML

Reinforcement Learning (MaskablePPO) für Bloons TD 6 auf einer eigenen, headless Simulation.
Ziel-Modus: **CHIMPS** auf Monkey Meadow. Später soll die gelernte Policy über eine
Screen-Capture-Bridge im echten Spiel spielen.

## Stand

- `engine/`: Rust-Engine (`btd-data`, `btd-core`, `btd-env`, `btd-tools`, `btd-py`), im f64-Build bit-genau zur Python-Referenz
- `btd_sim/`: Python-Referenz-Engine (vereinfachte Bloons, 4 Türme, prozedurale Runden) + Gymnasium-Env mit Action-Masking
- `train.py` / `evaluate.py`: MaskablePPO-Training und Auswertung (Faltungs-Policy, `btd_sim/policy.py`)
- `data/raw/towers/`: Turm-Daten aller BTD6-Affen aus dem Wiki (Rohformat, siehe `data/raw/towers/SCHEMA.md`)
- `data/base/towers/`: normalisierte Turm-Daten (erzeugt mit `tools/normalize.py`, Format: `data/schema/tower.schema.json`)
- `tools/wiki/`: Skripte, mit denen die Turm-Daten erzeugt wurden
- `docs/engine-design.md`: Design der geplanten Rust-Engine (`btd-core` + PyO3-Bindings)
- `mod/`: MelonLoader-Mod für BTD6, dumpt die Spieldaten (Türme inkl. aller Crosspaths, Bloons, Runden, Karten) nach `data/raw/game_dump/` (nicht im Repo), siehe `mod/README.md`

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

## Echte Spieldaten (lokal)

Mit dem Spiel-Dump der Mod (`mod/`, `data/raw/game_dump/`) erzeugt der Konverter eine
Spielkonfiguration für die Engine. Sie bleibt wie der Dump lokal (`data/local/`, gitignored):

```bash
.venv/bin/python -I tools/build_game_v1.py                  # Monkey Meadow, Medium, Runde 1-40
cd engine
cargo run --release -p btd-tools -- demo --data ../data/local/game_v1_tutorial_medium.json
# Benchmark (a): fester Bau-Plan (crates/btd-tools/src/meadow_plan.txt) schafft Runde 40
cargo run --release -p btd-tools -- script --data ../data/local/game_v1_tutorial_medium.json
cargo test --release -p btd-core --test dump_local -- --ignored
cargo test --release -p btd-tools --test benchmark -- --ignored
cd .. && .venv/bin/python train.py --data data/local/game_v1_tutorial_medium.json --preview 3 --envs 64
.venv/bin/python evaluate.py --model models/ppo_btd --data data/local/game_v1_tutorial_medium.json --preview 3
```

Standard-Policy ist die Faltungs-Policy aus `btd_sim/policy.py`: Raster und Skalare gehen durch
Residual-Faltungen, die Logits für Bauen/Upgrade/Verkaufen kommen pro Zelle direkt aus dem Netz.
Nach 3M Schritten (~1 h) gewann sie Monkey Meadow Medium bis Runde 40 in allen Testspielen (bisher
ein Seed), das flache MLP (`--arch mlp`) knapp die Hälfte bzw. 80 % (zwei Seeds) (Details in `docs/m3-log.md`). Die erreichte Runde steht
in Tensorboard unter `btd/`. Auf CUDA rechnet sie in bf16 und mit `torch.compile` (abschaltbar mit
`--no-amp`/`--no-compile`; compile braucht `python3.12-dev`), gemessen ~2 200 Schritte/s auf einer RTX 4060 Ti
(3M Schritte also rund 25 min).

Der Konverter schreibt neben der Datei einen Abdeckungs-Report (`*_coverage.md`): welche der 384
Upgrade-Kombinationen der 6 Primary-Türme die Engine simuliert und was den übrigen fehlt.
Nicht unterstützte Kombinationen sind im Aktionsraum gesperrt. `--preview N` hängt eine
Runden-Vorschau an die Observation (Bloon-Kategorien der nächsten N Runden, Runden bis zum
nächsten Camo/Lead/Ceramic/MOAB …).

## Spielprotokolle (Replays)

Mit `RustBTDEnv(record=True)` wird jedes Spiel als `.jsonl` mitgeschrieben (`save_replay`).
Ein Protokoll lässt sich nachspielen, gegen die Sim vergleichen oder ab einer Runde fortsetzen:

```bash
cargo run --release -p btd-tools -- replay spiel.jsonl            # strikt nachspielen
cargo run --release -p btd-tools -- replay spiel.jsonl --compare  # Abweichungen auflisten
```

Ein echtes Spiel kann man von Hand aufschreiben, `result` darf dabei unvollständig sein.
Positionen sind Weltkoordinaten der Karte (im sim_v0-Datensatz ist eine Rasterzelle 1×1, der
Mittelpunkt von Zelle (7, 4) ist also 7.5/4.5); Upgrade und Verkauf meinen den Turm an der Position:

```json
{"type":"header","format":"btd-replay/2","data":"sim_v0.game_v1.json"}
{"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7.5,"y":4.5}}],"result":{"lives":150}}
{"type":"round","round":2,"actions":[{"upgrade":{"x":7.5,"y":4.5,"path":0}}],"result":{"lives":150,"cash":512}}
```

## Setup

```bash
uv venv .venv
uv pip install --python .venv/bin/python torch --index-url https://download.pytorch.org/whl/cpu
uv pip install --python .venv/bin/python -r requirements.txt
.venv/bin/python evaluate.py --random
```

Datenquelle: [bloonswiki.com](https://www.bloonswiki.com) (Stand der Spieldaten ≈ v55–56).
