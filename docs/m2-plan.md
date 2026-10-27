# M2: Echte Physik und Daten

Ziel: Die Engine spielt Monkey Meadow mit den echten BTD6-Daten (Spiel-Dump 57.4) und den
6 klassischen Primary-Türmen (Dart, Boomerang, Bomb, Tack, Ice, Glue), Runden 1–40 auf Medium,
mit Projektilen statt Hitscan. Benchmark (a) aus dem Design: eine handgeschriebene Strategie
schafft Runde 40, danach ein kurzer PPO-Lauf.

Entscheidungen (2026-10-27):

- **sim_v0 bleibt als Konfiguration erhalten.** Die neue Engine ist so allgemein, dass der
  M1-Datensatz (Hitscan, 1-HP-Bloons, Raster, 20 Ticks/s) nur ein Datensatz im neuen Format ist.
  Der bit-genaue Paritätstest gegen Python bleibt dadurch als Regressionsschutz bestehen.
- **Türme stufenweise:** erst Basis + Tier 1–2 aller Pfade (inkl. Crosspaths), dann Tier 3,
  dann 4–5. Ein Abdeckungs-Report zeigt pro Kombination, welche Mechaniken fehlen. Nicht
  unterstützte Kombinationen sind im Aktionsraum gesperrt.
- **Öffentliches Repo:** Dump-Daten und alles daraus Abgeleitete bleiben lokal (`data/local/`,
  gitignored). CI testet mit synthetischen Fixtures im neuen Format plus sim_v0-Parität. Tests
  gegen den echten Dump laufen nur lokal (übersprungen, wenn kein Dump da ist).

## Datenformat `game_v1`

Ein JSON pro Spielkonfiguration (Karte + Runden + Türme + Regeln), Einheiten wie im Spiel
(Distanzen in Spiel-Einheiten, Zeiten in Sekunden).

- **Türme als Varianten pro Upgrade-Kombination.** Der Dump liefert jede gültige Kombination
  (64 pro Turm) als vollständiges Modell. Ein Upgrade wechselt auf die nächste Variante; Crosspaths
  müssen nicht aus Modifiern zusammengesetzt werden. Modifier (Design §5) bleiben für Buffs
  (Village, Alchemist, ab M4).
- Variante: Range, Camo, Footprint, Angriffe. Angriff: Range, Targeting, Waffen. Waffe: Cooldown,
  Emission (einzeln / Fächer / radial), Projektil. Projektil: Pierce, Schaden, Schadenstyp
  (welche Bloon-Eigenschaften es nicht treffen kann), Bonus-Schaden je Klasse, Radius, Bewegung
  (Hitscan / gerade mit Speed + Lifespan), Kind-Projektile bei Kontakt (Explosionen), Effekte
  (Slow, Freeze, Schaden über Zeit, Glue), später mehr.
- Bloons: HP, Speed, Leak-Schaden, Kinder (Typ + Anzahl), Eigenschaften (Lead, Black, White,
  Purple, Frozen-immun …), Klassen (MOAB, Ceramic). Camo/Regrow/Fortified sind Flags der Instanz.
- Runden: Gruppen mit Bloon-Typ, Flags, Anzahl, Start/Ende in Sekunden; Income-Set pro Runde.
- Regeln: Tick-Rate, Start-Cash/-Leben, Start-/Endrunde, Verkaufen, Kostenfaktor + Rundung,
  Kinder-Versatz. Presets Medium und CHIMPS aus `matches/` bzw. `gamedata/mods.json`.
- Karte: Pfad (Punkte + kumulierte Längen), Platzierungsflächen (Polygone, Land/Wasser/gesperrt),
  Spielfeldgrenzen, Raster für den Aktionsraum.

## Phasen

### M2a: Format und Engine-Umbau (Parität bleibt grün) ✅

1. `game_v1`-Typen in `btd-data` mit strenger Validierung (unbekannte Felder werden abgelehnt;
   die Rust-Typen sind die Schema-Definition).
2. `tools/export_sim_data.py` schreibt sim_v0 zusätzlich als `game_v1` (Hitscan, Raster als
   Koordinaten, Bloons mit 1 HP pro Schicht).
3. `btd-core` auf `game_v1` umbauen: HP-Bloons, Instanz-Flags, Varianten-Türme, kontinuierliche
   Platzierung mit Footprint-Kollision, konfigurierbare Tick-Rate, Angriffe als Behaviors
   (zunächst Hitscan/Splash/Radial wie bisher). Golden-Trace-Parität und Env-Parität bleiben grün.
4. Synthetische Fixture `tests/fixtures/game_v1_mini.json` für CI.

### M2b: Projektile und Dump-Konverter (Basis + Tier 1–2) ✅

1. `tools/build_game_v1.py --dump …` → `data/local/game_v1_meadow.json`: Monkey Meadow (Pfad,
   Flächen), DefaultRoundSet 1–40 + Income-Set, Medium-Regeln, alle Bloons, die 6 Türme.
   Übersetzt die Turm-Modelle in das `game_v1`-Angriffsschema; unbekannte gameplay-relevante
   Model-Typen landen im Abdeckungs-Report (Grafik/Sound werden ignoriert).
2. Engine: Projektile mit Flugbahn, Kollisionsradius, Pierce pro Projektil, Treffer-Sets,
   Explosionen bei Kontakt, Camo-Filter, Immunitäten, HP + Kinder (Ceramic, MOAB),
   Fortified, Regrow, Income.
3. Basis + Tier 1–2 von Dart, Tack und Bomb laufen (gerade Projektile, Fächer, Explosionen);
   Boomerang (Kurvenbahn), Ice (Einfrieren) und Glue (Verlangsamen) kommen mit den
   Status-Effekten in M2c. Der Abdeckungs-Report zeigt den Stand pro Kombination.

Stand nach M2b: `tools/build_game_v1.py` erzeugt Monkey Meadow (Medium, Runde 1–40) aus dem
Dump; 87 von 192 Kombinationen von Dart/Tack/Bomb laufen (Basis + Tier 1–2 vollständig, inkl.
Frag Bombs). Plausibilität (`btd-tools demo`, `tests/dump_local.rs`): nur Darts halten Runde
1–27 ohne Verlust und verlieren in Runde 28 an die Leads; nur Bombs verlieren 3 Leben an die
Camo-Green in Runde 24, halten die Leads und fallen in Runde 36. Erledigte Kalibrierpunkte:
Spawn-Abstand `(end − start)/(count − 1)` bestätigt (Runde 1: alle 55,3 Frames); Kinder kommen
als eigene Spiel-Varianten (Fortified/Camo stimmen dadurch automatisch).

### M2c: Tier 3, dann 4–5 ✅ (Stand: 204 von 384 Kombinationen)

Status-Effekte (Slow, Glue, Freeze, Schaden über Zeit), Spezialverhalten der höheren Tiers.
Abdeckungs-Report bis alle 64 Kombinationen der 6 Türme unterstützt sind (oder bewusst
ausgeschlossen mit Begründung).

Stand nach M2c: Engine kann Freeze (mit Schicht-Vererbung und Dauer-Faktor je Tag), zwei
Slow-Slots (Glue mit Stufe; ohne Stufe ein Slot je Effekt-ID wie Permafrost und Stun, alle
multiplikativ), Schaden über Zeit
(Corrosive Glue), Zurückstoßen, kritische Treffer, parallele Projektile, Treffer-Reset
(Glaives), Verlangsamungs-Zonen (Arctic Wind), Kurvenbahnen (Boomerang), Flächentreffer mit
eigenem Radius (Ring of Fire) und Splitter, die beim Verbrauch explodieren (Cluster Bombs).
Abdeckung pro Turm: Dart 39, Boomerang 24, Bomb 40, Tack 49, Ice 24, Glue 28 von je 64.

Bewusst offen (gesperrt, Grund im Report): Fähigkeiten (M4), Abpraller an Hindernissen und am
Rand (Dart 3xx–5xx), Retarget/Orbit (Boomerang 3xx–5xx), Kylie/MOAB Press (easePath,
ArriveAtTarget), Embrittlement/Shatter (Ice 4xx–5xx, 004/005), Glue 4xx–5xx (EmitOnDestroy),
Bloon Dissolver (DoT mit Ceramic-Bonus), Cluster mit FilterAll-Crosspaths (013/023/103/203).

### M2d: Env und Benchmark ✅

1. Aktionsraum aus dem Raster der Karte (Monkey Meadow: 7201 Aktionen); nicht unterstützte
   Kombinationen fehlen als Variante und sind dadurch maskiert.
2. Observation: Raster-Kanäle wie bisher, optional (`preview_rounds`) eine Runden-Vorschau:
   je Vorschau-Runde Anzahl Bloons in 9 Kategorien (Camo, Regrow, Fortified, Lead, Black, White,
   Purple, Ceramic, MOAB; inkl. Nachkommen), RBE und Dauer, dazu "Runden bis zur nächsten
   Kategorie" (Horizont 10). Ohne Vorschau bleibt die Observation identisch zur Python-Env.
3. Benchmark (a): `btd-tools script` spielt `crates/btd-tools/src/meadow_plan.txt` (Darts mit
   Camo, Tack, Boomerangs, Bombs, Glue) und gewinnt Medium Runde 1–40 mit 131/150 Leben
   (Verluste nur in Runde 37). Lokaler Test: `tests/benchmark.rs`.
4. Kurzer PPO-Lauf auf den Spieldaten (`train.py --data … --preview 3 --envs 64`, 600k Schritte,
   ~13 min auf der RTX 4060 Ti, ~800 Schritte/s inkl. Training): die Policy kommt deterministisch
   bis Runde 36 (4 von 5 stochastischen Episoden ebenso), baut dafür aber über 30 meist kleine
   Türme. Zufallspolicy: Runde ~6. Reward-Kurve flacht bei ~28 ab; für Runde 40 braucht es
   längeres Training und die Architektur-Experimente aus M3.

Offen für M3: Replay-Vergleich mit einem echten Testspiel des Users (Kalibrierung), die
Kalibrierpunkte unten, Regrow.

## Offene Kalibrierpunkte aus M2a

- Kostenrundung: `GameV1::cost` rundet half-up auf das Vielfache; C# `Math.Round` rundet
  standardmäßig half-to-even. Bei Hard (×1.08) gibt es keine Gleichstände, bei Easy (×0.85)
  schon (z.B. 50 → 42.5). Gegen die Preise im Dump prüfen.
- Spawn-Abstand in Gruppen: `(end − start) / (count − 1)`; im Spiel evtl. `/ count`. Gegen die
  Runden im Dump prüfen.
- Flag-Vererbung: Kinder erben alle Flags; im Spiel sind z.B. Kinder von Fortified-Lead/Ceramic
  nicht fortified.

## Offene Kalibrierpunkte aus M2b–M2d

Annahmen, die gegen das Spiel (Testspiel/Replay oder dekompilierten Code) zu prüfen sind; im
Code mit [KAL] markiert:

- Freeze-Schichten: `layers` zählt inklusive des getroffenen Bloons, Kinder erben mit einer
  Schicht weniger. Evtl. friert Basis-Ice dadurch eine Generation zu viel.
- `cascadeMutators` (Freeze/Permafrost true, Glue false) wird nicht ausgewertet.
- Slows verschiedener Effekt-IDs (Glue, Permafrost, Stun) wirken multiplikativ; im Spiel evtl. nur
  der stärkste.
- Schaden über Zeit trifft alle Eigenschaften; Immunitäts-Bit 64 (unbekannt) wird ignoriert.
- Pierce wird auch von immunen Bloons verbraucht.
- Zufallsreichweite (Cluster) und Crit-Intervall als Mittelwert.
- Projektile starten in der Turmmitte (Abwurf-Versatz ignoriert), Boomerang-Kurven werden nicht
  gespiegelt (FlipFollowPath), keine Sichtlinie, Explosionen sofort statt nach 0,1 s.
- Bonus-Schaden (`damage_vs`) geht als Überschuss an die Kinder weiter.
- Verbrauchs-Effekte (`on_exhaust`) lösen auch beim Ablaufen der Lebensdauer aus (Cluster-Splitter
  kollidieren im Spiel nicht: dontUseCollisionChecker).
- Negative Spawn-Zeiten (CHIMPS Runde 86) werden auf 0 geklemmt.

Nach jeder Phase: Commit, unabhängiger Review-Agent, Funde fixen, vor dem Push fragen.
