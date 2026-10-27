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

### M2b: Projektile und Dump-Konverter (Basis + Tier 1–2)

1. `tools/build_game_v1.py --dump …` → `data/local/game_v1_meadow.json`: Monkey Meadow (Pfad,
   Flächen), DefaultRoundSet 1–40 + Income-Set, Medium-Regeln, alle Bloons, die 6 Türme.
   Übersetzt die Turm-Modelle in das `game_v1`-Angriffsschema; unbekannte gameplay-relevante
   Model-Typen landen im Abdeckungs-Report (Grafik/Sound werden ignoriert).
2. Engine: Projektile mit Flugbahn, Kollisionsradius, Pierce pro Projektil, Treffer-Sets,
   Explosionen bei Kontakt, Camo-Filter, Immunitäten, HP + Kinder (Ceramic, MOAB),
   Fortified, Regrow, Income.
3. Basis + Tier 1–2 der 6 Türme laufen; Abdeckungs-Report zeigt den Stand.

### M2c: Tier 3, dann 4–5

Status-Effekte (Slow, Glue, Freeze, Schaden über Zeit), Spezialverhalten der höheren Tiers.
Abdeckungs-Report bis alle 64 Kombinationen der 6 Türme unterstützt sind (oder bewusst
ausgeschlossen mit Begründung).

### M2d: Env und Benchmark

1. Aktionsraum: Raster-Zelle → geprüfte Weltposition (Platzierungsflächen + Footprint),
   Masken nur für unterstützte Kombinationen.
2. Observation: Raster-Kanäle + Runden-Vorschau inkl. „Runden bis zum nächsten Spezial-Bloon“.
3. Benchmark (a): handgeschriebene Strategie schafft Medium Runde 40; Replay-Vergleich mit einem
   echten Testspiel des Users (Kalibrierung); kurzer PPO-Lauf.

## Offene Kalibrierpunkte aus M2a

- Kostenrundung: `GameV1::cost` rundet half-up auf das Vielfache; C# `Math.Round` rundet
  standardmäßig half-to-even. Bei Hard (×1.08) gibt es keine Gleichstände, bei Easy (×0.85)
  schon (z.B. 50 → 42.5). Gegen die Preise im Dump prüfen.
- Spawn-Abstand in Gruppen: `(end − start) / (count − 1)`; im Spiel evtl. `/ count`. Gegen die
  Runden im Dump prüfen.
- Flag-Vererbung: Kinder erben alle Flags; im Spiel sind z.B. Kinder von Fortified-Lead/Ceramic
  nicht fortified.

Nach jeder Phase: Commit, unabhängiger Review-Agent, Funde fixen, vor dem Push fragen.
