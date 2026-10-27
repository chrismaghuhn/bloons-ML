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

## 2026-10-27: Weniger Türme (`--reward-tower -0.5`) und erster Abgleich mit dem echten Spiel

Conv, 3M Schritte, Seed 1, −0,5 Reward je gebautem Turm (Upgrades frei). Lernt schneller als
ohne Strafe (Ø Runde 40 schon bei ~1M Schritten statt 2,6M) und spielt mit Ø 2 Türmen:
stochastisch 19/19 gewonnen, 150 Leben. Deterministischer Build:

| Wann | Aktion |
|---|---|
| vor Runde 1 | Bomb Shooter bei (15, −31), Dart Monkey bei (−5, −1) |
| vor Runde 6 | Dart 0-0-1, 0-0-2 |
| vor Runde 7 | Dart 0-0-3 |
| vor Runde 16 | Dart 0-0-4 (Sharp Shooter), danach nichts mehr (Ende: 14 016 $ übrig) |

Im echten Spiel nachgespielt (BTD6, Monkey Meadow, Medium, Targeting First): **Runde 40 geschafft.**
Bei Runde 15 noch 150 Leben, Upgrade-Zeitpunkte passten zum Geld; Leben am Ende nicht notiert.
Erster Hinweis, dass die Sim für diesen Build nicht zu großzügig ist. Nächster Abgleich: Leben nach
Runde 40 und Freeplay bis zum Verlust (Daten für Runde 41–60 fehlen der Sim noch).

## 2026-10-27: Training schneller (bf16, channels_last, torch.compile)

Die Zeit pro PPO-Iteration (32k Schritte, 64 Envs) steckt zu ~85 % im Update (6 Epochen Backprop
durch das Conv-Netz), nicht in der Engine. Gemessen mit Conv 32×4, Batch 1024, alles andere gleich
(die f32-Zeile lief parallel zu BTD6 auf derselben GPU, ist also etwas zu langsam):

| Variante | Rollout | Update | Schritte/s |
|---|---|---|---|
| f32 | 3,9 s | 27,2 s | ~1 050 |
| bf16 + channels_last (`--amp`, Standard) | 4,4 s | 13,8 s | 1 800 |
| + torch.compile (`--compile`, Standard) | 3,7 s | 11,2 s | **2 200** |
| + 256 statt 64 Envs | 2,3 s | 11,7 s | 2 330 (ändert die Rollout-Länge, nicht übernommen) |

bf16 lernt gleich gut: Lauf mit Seed 1 erreicht Ø Runde 40 erstmals bei 1,44M und stabil ab 1,57M
Schritten (f32: 2,6M, Streuung zwischen Läufen), 19/19 Siege. 3M Schritte dauerten 28 min (ohne
compile, GPU für sich); der f32-Lauf brauchte 57 min, teilte sich die GPU aber mit dem MLP-Lauf. torch.compile braucht die
Python-Header (`sudo apt install python3.12-dev`); ohne sie rechnet `train.py` mit einer Warnung
unkompiliert weiter. Schneller ginge es nur noch mit weniger Epochen oder kleinerem Netz, was das
Lernen verändert.

## 2026-10-27: Festhäng-Metrik (vor der Auswertung festgelegt)

Frage: Erzeugen die Turm-Sperren (`--ban-towers 2`) echte Vielfalt oder nur Notfallpläne je
Sperr-Kombination? Gemessen in Spielen **ohne Sperre** (k = 0), stochastische Policy, 64 Episoden je
Modell, gleiche Daten (Medium R60):

- Je Episode ein Schlüssel = (erster gebauter Turmtyp, Menge der (Turmtyp, Haupt-Upgrade-Pfad) aller
  gebauten Türme). Haupt-Pfad = Pfad mit der höchsten Stufe, bei Gleichstand der kleinere Index,
  "-" ohne Upgrade.
- Kennzahl: Shannon-Entropie H (bit) der Verteilung dieser Schlüssel.
- **Kriterium:** Ist H beim Sperren-Lauf nicht mindestens **0,5 bit** höher als bei der Baseline
  (`r60_base`), werten wir die Sperren als "nur Notfallpläne".
- Zusätzlich berichtet (nicht Teil des Kriteriums): Verteilung erster Turm, Haupt-Pfade, Runde des
  ersten Turms je Typ, belegte Zellen.

Überleben je Sperr-Kombination (keine, 6 Einzel-, 15 Zweier-Sperren): erreichte Runde und Leben
deterministisch (1 Spiel) sowie Ø Runde und Siegquote stochastisch (10 Spiele).
