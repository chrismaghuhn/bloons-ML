# M3: Protokoll

Messungen und Experimente aus M3 (ernsthaftes Training), neueste unten.

## 2026-10-27: Engine schneller (01d9900)

Monkey Meadow Medium (`data/local/game_v1_tutorial_medium.json`), `btd-tools bench`, f32:

| | vorher | nachher |
|---|---|---|
| 1 Env, Env-Schritte/s | 204 | 4 700 |
| Aktionsmasken/s | 224 | 245 000 |
| Runden/s (Zufallspolicy) | ~420 | ~1 000 |
| VecEnv 64, Env-Schritte/s | 1 220 | 19 900 |

Die Maske prüfte in jedem Schritt die Karten-Polygone für alle 720 Zellen × 6 Türme; das ist
jetzt einmal vorberechnet. Dazu gecachte Bloon-Positionen, `compact()` nur ab dem ersten toten
Bloon und billige Abstandstests zuerst. Alles bit-gleich (sim_v0-Parität, Hash über
Zufallsspiele). Das Training ist jetzt durch Python/PyTorch begrenzt, nicht mehr durch die Engine.

## 2026-10-27: MLP vs. Faltungs-Policy (668d1df)

Gleiche PPO-Einstellungen (`train.py --preview 3 --envs 64 --steps 3000000`), Seed 1, RTX 4060 Ti.
Ø/Max erreichte Runde der letzten 100 Trainings-Episoden:

| Schritte | MLP [512, 256] | Conv (32 Kanäle, 4 Blöcke) |
|---|---|---|
| 262k | 30,9 / 39 | 32,6 / 39 |
| 524k | 34,8 / 39 | 36,0 / 39 |
| 1,05M | 38,7 / 39 | 36,1 / 39 |
| 1,57M | 39,2 / 40 | 36,4 / 39 |
| 2,10M | 39,3 / 40 | 39,1 / 40 |
| 2,62M | 39,4 / 40 | **40,0 / 40** |
| 3,01M | 39,3 / 40 | **40,0 / 40** |

Auswertung der fertigen Modelle (1 deterministische + 19 stochastische Episoden; MLP Seed 2: 20 stochastische):

- **Conv:** deterministisch gewonnen (136 Leben), stochastisch 19/19 gewonnen, Ø 148,9 Leben,
  Ø 25 Türme. Besser als der feste Skript-Plan (131 Leben).
- **MLP:** deterministisch nur Runde 8 (Ursache nicht untersucht), stochastisch
  9/19 gewonnen, Ø 48 Leben, Ø 42 Türme. Zweiter MLP-Seed: ähnliche Kurve, stochastisch 16/20 gewonnen.

Durchsatz inkl. Training (beide Läufe parallel auf einer GPU): MLP ~1 700, Conv ~900 Schritte/s;
3M Schritte dauerten mit Conv ~57 min. Mit 64 Kanälen/6 Blöcken waren es nur ~500 Schritte/s.

Folgerung: Conv ist der neue Standard mit der Rust-Engine (Python-Engine: MLP). Conv ist bisher
nur mit einem Seed gelaufen. Offen: weitere Seeds für Conv, nächste
Benchmark-Stufe (Runde 60), deterministisches vs. stochastisches Spielen für die Bridge.
