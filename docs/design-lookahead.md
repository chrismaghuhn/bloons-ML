# Design: Vorausschau beim Spielen (umgesetzt 2026-10-27)

Idee (vom User): Bevor der Bot eine Runde startet, prüft er in der Sim, ob er Leben verlieren würde,
und probiert sonst bessere Aktionen. Anlass: Im echten Spiel hielt ein vorgezogener Crossbow Master
Runde 45 ohne Leak; der Bot hortete 19 000 $ und verlor dort 78 Leben, weil frühe Upgrades in seiner
Belohnung kaum zählen.

## 1. Ablauf (nur beim Spielen, kein Training)

In jeder Bauphase:

1. Die Policy schlägt eine Aktion vor. Ist es eine Bau-/Upgrade-/Verkaufsaktion, wird sie ausgeführt
   wie bisher.
2. Will sie die Runde starten (Aktion 0), simuliert die Vorausschau auf einer **Kopie** des Spiels die
   nächsten `H` Runden (ohne weitere Aktionen) und zählt die verlorenen Leben.
3. Ohne Verlust: Runde starten. Mit Verlust: die `k` wahrscheinlichsten anderen erlaubten Aktionen der
   Policy (Maske beachtet) je einzeln ausprobieren — Aktion anwenden, dann `H` Runden simulieren — und
   die mit den wenigsten verlorenen Leben nehmen (Gleichstand: mehr übriges Geld, dann höhere
   Policy-Wahrscheinlichkeit). Ist
   keine besser als "starten", wird gestartet.
4. Nach einer gewählten Aktion zurück zu 1 (die Bauphase kann bis zum Bau-Limit weitergehen).

Vorschlag Start: `H = 2`, `k = 8`. Kosten: höchstens (k + 1) · H Sim-Runden je Rundenstart, also ~20
Runden à ~1 ms; ein Spiel bis Runde 60 dauert damit ~1-2 s statt ~0,1 s.

## 2. Umsetzung

- Rust (`btd-env`): `Env::lookahead(action: Option<usize>, rounds: u32) -> Lookahead { lives_lost,
  rounds_played, cash, game_over, invalid }` auf einer Kopie (`Env` ist `Clone`; Zufall: gleicher
  Zustand wie das echte Spiel der Env, damit die Vorhersage zur Episode passt); deterministisch, ändert
  die Env nicht. Mit Zufall kennt die Vorausschau damit die künftigen Crits exakt; im echten Spiel
  (Bridge) ist der Zufall unbekannt, dort mit mehreren Seeds vorausschauen.
- Python: `btd_sim/lookahead.py` mit `LookaheadPolicy(model, env, k, horizon)`, die wie `model.predict`
  benutzt wird (Top-k aus den Logits der Policy inkl. Maske).
- `evaluate.py` / `tools/analyze_policy.py`: Option `--lookahead K,H`.

## 3. Messen

Mit und ohne Vorausschau, gleiches Modell (`t12_r60_v2`), deterministisch:
- Siege, Leben und erreichte Runde ohne Sperre und je Einzelsperre,
- Eval-Satz "Erholung",
- CHIMPS-Daten (1 Leben), sobald ein Modell dafür trainiert ist,
- Rechenzeit je Spiel.

Erwartung: deutlich weniger verlorene Leben, vor allem in Runden mit Spezial-Bloons (z.B. Runde 45).

## 4. Grenzen und spätere Ausbauten

- Die Vorausschau ist nur so gut wie die Sim (Kalibrierung mit dem Recorder bleibt wichtig). Für das
  echte Spiel optional ein **Sicherheitspuffer**: zusätzlich mit etwas schnelleren Bloons simulieren
  und nur starten, wenn auch dann nichts leakt.
- `H` Runden ohne weitere Aktionen ist vorsichtig (unterschätzt die spätere Stärke); reicht für die
  Frage "verliere ich in der nächsten Runde Leben?".
- Später (größer): Suche fürs Training (Expert Iteration / MCTS wie AlphaZero), damit die Policy das
  Gelernte ohne Suche kann. Erst nach Messung von Schritt 1 entscheiden.
- Bridge: dieselbe Vorausschau läuft vor jedem Rundenstart im echten Spiel auf dem Sim-Abbild des
  Spielstands (Startzustand aus der Mod).

## 5. Offene Fragen

1. Startwerte `H = 2`, `k = 8` ok?
2. Nur bei "Runde starten" prüfen (Vorschlag), oder auch einzelne Bauaktionen der Policy gegenprüfen?
3. Zählt nur "Leben verloren", oder bei Gleichstand auch übrig bleibendes Geld (spart unnötige Käufe)?

## 6. Entschieden und gemessen

Entschieden: `H = 2`, `k = 8`, nur bei "Runde starten" prüfen, bei gleich vielen verlorenen Leben mehr
übriges Geld bevorzugen. Umgesetzt: `Env::lookahead` (Rust, Kopie mit gleichem Zufallszustand),
`btd_sim/lookahead.py` (`LookaheadPolicy`), `evaluate.py`/`tools/analyze_policy.py --lookahead 8,2`.

Messung mit `t12_r60_v2` (12 Türme, R60, korrigierte Sim), deterministisch:

| | ohne Vorausschau | mit Vorausschau |
|---|---|---|
| ohne Sperre | gewonnen, 72 Leben | gewonnen, 150 Leben |
| Einzelsperren gewonnen (von 12) | 7 | 11, alle mit 150 Leben |
| ohne Dart | Runde 9 | Runde 47 |
| Erholung (von 50) | 46 | 50 |

Rechenzeit der ganzen Auswertung 30 s statt 25 s.
