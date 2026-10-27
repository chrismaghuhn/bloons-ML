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

### Ergebnis (r60_base vs. r60_bans, beide Conv, `--reward-tower -0.5`, Seed 1)

`r60_bans`: 5M Schritte, `--ban-towers 2`. Vielfalt ohne Sperre (64 Spiele, stochastisch):

| | H(Schlüssel) | häufigster Build | Anteil |
|---|---|---|---|
| r60_base | 0,35 bit | erst Dart; Dart unten + Bomb unten | 61/64 |
| r60_bans | 0,53 bit | erst Boomerang; Boomerang unten + Dart unten | 58/64 |

**+0,18 bit < 0,5 bit: nach dem Kriterium nur Notfallpläne.** Die Sperren haben das Lieblingsrezept
gewechselt, nicht vervielfacht.

Überleben je Sperr-Kombination (stochastisch, 10 Spiele): r60_base gewinnt 11 von 22 Kombinationen
(genau die mit Dart und Bomb), r60_bans 15 von 22. Ohne Dart gewinnt keiner (r60_bans kommt bis
Runde 36, r60_base bis 3); ohne Bomb gewinnt r60_bans stochastisch 10/10, deterministisch aber nur bis
Runde 29. Fazit: Sperren machen robuster, aber nicht vielfältiger. Nächster Hebel: Startzustände
(docs/design-start-states.md).

## 2026-10-27: Startzustände (Schritt 1+2 umgesetzt), Vorher-Werte

Eval-Satz "Erholung": 50 gestörte Zustände (Runde 3-57, Ø 29,3) aus eigenen Eval-Replays von
r60_base/r60_bans (nicht die Trainings-Replays der Quelle B), Seed 2026.

| Modell | deterministisch | stochastisch |
|---|---|---|
| r60_base | 34/50 Siege, Ø 21,2 Runden weiter | 37/50, Ø 22,0 |
| r60_bans | (39/50, ungültig: zufällige Sperren mitgewürfelt) → neu: 40/50, Ø 26,3 | 43/50, Ø 26,8 |

Bekannte Grenze der Gewichtung: Bei ~7 000 Pool-Zuständen wird jeder Zustand nur selten gezogen,
A-Zustände fallen nach 2 000 neuen wieder raus. Die 3-Runden-Gewichtung greift daher kaum; der Anteil
"sofort verloren" wird vor allem von den Störungen bestimmt (geloggt unter `starts/`).

### Ergebnis r60_starts (wie r60_bans, plus `--start-states 0.3`)

5M Schritte, Seed 1, Quelle B: 100 Replays von r60_base/r60_bans (5 641 Zustände).
Training: 30 % der Episoden, aber nur ~18 % der Schritte aus Startzuständen (15 % Pool, 3 % zufällig);
"sofort verloren" sank von 37 % auf 10 %, keine Fehlstarts. Normale Starts (Ø Runde, `starts/normal_round`)
am Ende gleichauf mit r60_bans (55,6 vs. 54,1 bei 4,7M).

| | r60_base | r60_bans | r60_starts |
|---|---|---|---|
| Erholung det. / stoch. (von 50) | 34 / 37 | 40 / 43 | **48 / 48** |
| H(Schlüssel) ohne Sperre | 0,35 bit | 0,53 bit | 0,82 bit |
| Sperr-Kombinationen gewonnen, stoch. / det. (von 22) | 11 / 11 | 15 / 13 | 15 / 15 |

- **Erholung:** klarer Gewinn, aus gestörten Zuständen fast immer gewonnen.
- **Vielfalt:** +0,47 bit gegenüber der Baseline, **Kriterium (+0,5) knapp verfehlt**; 55/64 Spiele
  spielen weiter ein Rezept (Bomb unten + Dart unten). Ein Seed, 64 Spiele: der Abstand zur Schwelle
  liegt im Rauschen, das Kriterium bleibt trotzdem so stehen.
- **Ohne Dart** gewinnt weiterhin kein Modell (r60_starts: Ø Runde 36).

## 2026-10-27: 12 Türme (t12_r60)

Daten `--towers t12` (275/768 Kombinationen), R60, 5M Schritte, Seed 1, `--reward-tower -0.5
--ban-towers 3 --start-states 0.3`. ~1 900 Schritte/s. Normale Starts: schneller als mit 6 Türmen
(Ø Runde 55,4 bei 1M statt 49,2), Spitze 57,3 bei 2,1M, am Ende ~53.

- **Vielfalt ohne Sperre:** H = 5,46 bit, 50 Schlüssel in 64 Spielen (6 Türme: 0,82 bit). Formal weit
  über dem Kriterium, aber nur teilweise echte Vielfalt: erster Turm in 58/64 Sniper, Sniper oben 92 %,
  Dart unten 100 %; die Entropie kommt vor allem von wechselnden Neben-Türmen (oft ohne Upgrade).
  Das Kriterium war für 6 Türme gedacht; für Vergleiche über Turmzahlen braucht es einen Kern-Schlüssel
  (z.B. die zwei Türme mit den höchsten Ausgaben).
- **Sperren** (stochastisch, von 10): keine 8, ohne Dart 2 (6-Turm-Modelle: 0), ohne Sniper 6, sonst 6-10.
- **Deterministisch schwach:** ohne Sperre Runde 26, viele Sperren Runde 12; Erholung det. 27/50 vs.
  stoch. 43/50; auch stochastisch ohne Sperre nur 55/64 Siege (6 Türme: 64/64). Die Policy verteilt
  ihre Wahrscheinlichkeit breit, die wahrscheinlichste Einzelaktion ist oft schlecht.
- Nächster Schritt: Entropie-Schedule (abklingender ent_coef) als getrennter Lauf mit sonst gleicher
  Konfiguration.

### Abgleich echtes Spiel, Teil 2: Zwei-Turm-Build über Runde 40 hinaus

Dart 0-0-4 bei (−5, −1) + Bomb 0-0-0 bei (15, −31), ab Runde 16 keine Aktionen mehr (Freeplay bis 60).

| Runde | Sim (medium60): Leben danach | echtes Spiel |
|---|---|---|
| ≤ 42 | 150 | 150 |
| 43 | 91 (erste Leaks) | erste Leaks |
| 44 | 91 | 65 (Screenshot während/nach Runde 44) |
| 45 | 13 (−78) | **Defeat, Leak-Anzeige −78** (Leads; nur noch 65 Leben) |
| 47 | 0 | – |

Erste Leak-Runde stimmt exakt, der Verlust in Runde 45 (verstärkte Leads) auf das Leben genau (−78).
Abweichung nur in Runde 43/44 (echtes Spiel ≥ 26 Leben mehr verloren), dort hängt das Ergebnis stark
am Crit-Timing (Sim mit festem Abstand 8-12: Leben nach Runde 44 zwischen 89 und 150).
Passt zum Verdacht, dass die Sim leicht großzügig ist (keine Sichtlinie, Crits exakt jeder 10. Schuss).

## 2026-10-27: Entropie-Schedule (t12_r60_ent)

Wie t12_r60, plus `--ent-coef 0.01 --ent-coef-end 0.001` (linear). Entropie der Policy fiel in den
letzten 500k Schritten von ~2 auf 0,96 (ohne Schedule ~2); normale Starts am Ende Ø Runde 56,1 (53,3).

| | t12_r60 | t12_r60_ent |
|---|---|---|
| Siege ohne Sperre (stoch., 64) | 55 | **64** |
| Einzelsperren mit 10/10 Siegen (von 12) | 1 | 10 (ohne Sniper 9/10) |
| ohne Dart (stoch., 10) | 2 | 0 |
| deterministisch ohne Sperre | Runde 26 | Runde 46 |
| Erholung det. / stoch. (50) | 27 / 43 | 33 / 41 |
| H(Schlüssel) ohne Sperre | 5,46 bit (50) | 2,09 bit (13) |

Stochastisch deutlich stärker und zuverlässiger; deterministisch besser, aber weiter schwach (in den
meisten Sperren Runde 26-39). Vielfalt sinkt: immer zuerst Sniper (oben), dann Dart (unten).
Offen: warum Argmax scheitert (einzelne Spiele ansehen).

Nachtrag: Im Dump 57.4 ist der Crit-Abstand des Sharp Shooters fest 10 (lower = upper = 10), also
kein Crit-Glück im echten Spiel. Die Abweichung in Runde 43/44 hat eine andere Ursache; Kandidaten:
Sichtlinie (Bäume blocken Schüsse, die Sim kennt keine Hindernisse), Details der Zielwahl.

## 2026-10-27: Kalibrierung Track-Länge und Sniper (widersprüchlich, noch keine Änderung)

Echte Messungen (Monkey Meadow, Medium): Runde 1 ohne Türme dauert 50,0 s (Stoppuhr); der Bot-Build
(Sniper 0-0-0 rechts bei (105, −41)) verliert schon in Runde 1 3 Leben.

| Sim-Variante | R1 ohne Türme | Sniper R1 Leaks | Zwei-Turm erste Leaks | Leben nach R44 | Verlust R45 |
|---|---|---|---|---|---|
| echtes Spiel | 50,0 s | 3 | R43 | 65 | 78 |
| voller Track (Dump, 1276 E.) | 68,6 s | 0 | R43 | 91 | 78 |
| nur sichtbar (894 E.) | 53,3 s | 0 | R38 | 146 | 78 |
| sichtbar + 20 E. | 54,9 s | 0 | R45 | 150 | 129 |

- Der Dump-Track reicht 183 E. links und 199 E. unten über den Bildrand; die Laufzeit spricht dafür,
  dass Bloons diese Abschnitte im Spiel nicht (voll) laufen.
- Kürzen verschlechtert aber den Zwei-Turm-Abgleich: dessen exakte Treffer waren wohl teils Zufall (das
  Ergebnis springt schon bei kleinen Änderungen, vgl. Crit-Abstände).
- Die Sniper-Leaks in R1 erklärt keine Variante: der Sniper ist in der Sim stärker (Kandidaten:
  Verzögerung vor dem ersten Schuss, echte Schussrate, Platzierung erst während der Runde).
- Sichtlinie scheidet hier aus: 119 Hindernis-Kreise (BlockerModel) liegen nur in den Ecken, der
  Sniper sieht 100 % des Tracks. Der Konverter ignoriert sie bisher; für andere Plätze/Karten nötig.
- Vorschlag: Bloon-Positionen und Schüsse im echten Spiel per Mod mitschreiben und Tick für Tick
  vergleichen, statt aus Endständen zurückzurechnen.

## 2026-10-27: Laufstrecke und Bloon-Tempo korrigiert (Recorder-Mod, Schritt 1)

Die Recorder-Test-Mod (docs/design-recorder.md §6) hat gemessen: Bloons erscheinen auf Monkey Meadow
bei Pfad-Distanz 173,6 und leaken bei 1085,67 (Laufstrecke 912 statt der 1276 des gedumpten Pfads);
die Schwierigkeit skaliert das Tempo aller Bloons (GlobalSpeedModModel: Medium 1,1, Hard/CHIMPS 1,25).
Der Konverter kürzt den Pfad jetzt auf diese Strecke und liest den Faktor aus den Partie-Regeln.

| Messpunkt | echt | Sim vorher | Sim jetzt |
|---|---|---|---|
| Runde 1 ohne Türme | 50,68 s (Mod) / 50,0 s (Stoppuhr) | 68,6 s | 50,68 s |
| Zwei-Turm-Build: erste Leaks | Runde 43 | Runde 43 | Runde 43 |
| Zwei-Turm-Build: verloren in | Runde 45 (−78) | Runde 47 | Runde 45 |
| Zwei-Turm-Build: Leben nach R44 | 65 | 91 | 131 |
| Sniper 0-0-0 allein, Runde 1 | 3 Leaks | 0 | 0 |

Offen: Runde 43 kostet im Spiel mehr; der Sniper (rechnerisch müsste er ~20 s später anfangen oder
deutlich langsamer feuern) -> Schüsse mit der Recorder-Mod aufzeichnen. Alle bisher trainierten Modelle
beruhen auf den alten Daten (Bloons ~1,5x zu langsam) und müssen neu trainiert werden; der feste
Skript-Plan (Benchmark a) gewinnt Runde 40 weiterhin (132 Leben).

Nachtrag 040c8a6 (game_v1 `meta`/`game_id`): neu erzeugte Datendateien brauchen eine neu gebaute
Python-Erweiterung (`maturin develop`). Ihr Daten-Hash ändert sich, daher passen ältere Replays in
`data/local/starts|eval|calib` beim strikten Nachspielen (`btd-tools replay`) nicht mehr; Startzustände
und Eval-Sätze prüfen den Hash nicht.
