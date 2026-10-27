# Design: Replay-Player in der Mod (freigegeben 2026-10-27; Schritt 1 erledigt, siehe §10)

Ziel: Ein Spiel aus der Sim (Replay `btd-replay/2`, später `/3`) oder ein Startzustand (`StartState`) wird im
echten BTD6 **exakt** ausgeführt: Türme an die exakten Koordinaten, Upgrades und Verkäufe zum richtigen Zeitpunkt,
Runden starten. Der Recorder (`docs/design-recorder.md`) läuft dabei mit. So lässt sich ein Sim-Spiel 1:1 im Spiel
abspielen und Tick für Tick vergleichen, statt aus Screenshots oder Endständen zurückzurechnen.

Baut auf der vorhandenen Mod auf (`mod/BloonsMLMod`): neuer Ordner `Player/`, gemeinsame Teile mit dem Recorder in
`Common/`. Stand der Prüfung: BTD6 57.4, Mod Helper 3.6.10. Signaturen unten stammen aus den generierten
Il2Cpp-Assemblies. Was zunächst nur aus Signaturen gefolgert war, hat Schritt 1 (Test-Mod) im Spiel geprüft; die
Abschnitte 1–4 geben den gemessenen Stand wieder, Einzelheiten in §10.

## 1. Spiel-APIs

Alles läuft über die Bridge `InGame.instance.bridge` (`UnityToSimulation`). `inputId` ist im Einzelspiel
`bridge.GetInputId()`.

| Zweck | Aufruf |
|---|---|
| Turm setzen | `CreateTowerAt(inputId, Vector2 pos, TowerModel tm, ObjectId forTowerId, bool isInstaTower, Action<bool> callback, bool ignoreInventoryChecks, bool ignorePlacementChecks, bool isEditorTower, int costOverride, bool deductCash, int frontierId)` |
| Platz prüfen | `CanPlaceTowerAt(pos, tm, inputId, forTowerId)` → bool; `QueryPlacementPosition(pos, tm, inputId, forTowerId)` → `PlacementQuery { position, positionValid, … }` |
| Upgrade | `UpgradeTower(inputId, ObjectId id, int pathIndex, double nonUpgradeCashInvestment, Action<bool> callback)` |
| Verkaufen | `SellTower(inputId, ObjectId id)` |
| Runde starten | `StartRound()`; Zustand: `AreRoundsActive()`, `CanSendNextRound()`, `GetCurrentRound()` |
| Runde setzen | `SetRound(int round, bool fromSave)`, `ResetRound()`, `SetEndRound(int)`; Mod Helper: `map.spawner.SetRound(round)` |
| Geld | `SetCash(double value, inputId)`, `GetCash(inputId)`; Mod Helper `InGame.SetCash` setzt `cashManager.cash.Value` |
| Leben | `simulation.health.Value` / `maxHealth.Value` (Mod Helper `SetHealth`, `SetMaxHealth`); Sandkasten: `SetSandboxHealth(float)` |
| Kosten | `GetTowerCost(tm, inputId)`, `GetUpgradeCost(id, path, tier, …)`, `IsUpgradeLocked(id, path, tier)` |
| Vorspulen | `SetFastForward(bool)` (Zeitbasis ist fest, `docs/design-recorder.md` §6: Vorspulen ändert nichts am Ablauf) |
| Targeting | `SetNextTowerTargetType(inputId, id, prev)` (schaltet eine Option weiter) |
| Türme lesen | `GetAllTowers()` → `TowerToSimulation`, `GetTowerFromId(id)` |

**Ausführungszeitpunkt (gemessen, §10):** `CreateTowerAt` und `UpgradeTower` gehen **immer in die Warteschlange**
`bridge.actions`, sowohl aus `OnUpdate` als auch aus dem `Simulate`-Prefix: kein Callback während des Aufrufs,
danach 1 Eintrag in der Warteschlange. Ausgeführt werden sie vor dem nächsten Sim-Schritt (zwischen den Runden: im
nächsten Frame mit derselben Sim-Zeit; in der Runde: ein Schritt später, siehe Abschnitt 4). `SetCash`, `SetRound` und
`ResetRound` haben keine `_Impl`-Variante und wirken sofort. Der Player wartet deshalb nach jeder Aktion, bis ihr
Callback gefeuert hat **und** `bridge.actions` leer ist, bevor er weitermacht. (Mod Helpers Tests lesen das Ergebnis
direkt nach dem Aufruf und warnen "could not be immediately performed", wenn etwas in der Warteschlange liegt; im
normalen Spiel ist das der Regelfall.)

**Wie Mod Helper es selbst macht:** `Api/Testing/ModTest.CreateTowerAt` ruft `CreateTowerAt` mit
`ignoreInventoryChecks`/`ignorePlacementChecks` auf und nimmt den neuen Turm als Differenz von `GetAllTowers()` vorher
und nachher; `ignoreInventoryChecks` greift laut Mod Helper nicht, dafür setzt es einen eigenen Patch
(`TowerInventory_HasInventory.overrideValue`). Wir rufen ohne `ignore…` auf, damit das Spiel genau so prüft wie bei
einem Spieler.

**Platzierungsfehler erkennen (vor, bei und nach der Aktion):**

1. Vorher: `QueryPlacementPosition` → `positionValid`; dazu Kosten gegen `GetCash(inputId)`.
2. Callback `Action<bool>` von `CreateTowerAt`/`UpgradeTower` (Erfolg ja/nein).
3. Nachher: neuer Turm in `GetAllTowers()`, seine `Position` gleich der angefragten (Toleranz 1e-3). Weicht sie ab
   (das Spiel könnte einrasten) oder fehlt der Turm, ist das ein Fehler.
4. Nach jedem Upgrade/Verkauf: Stufen des Turms bzw. Turm weg; Cash vorher/nachher gegen die erwarteten Kosten.

Jeder Fehler geht in die Ergebnisdatei (Abschnitt 5) und ins Log; der Player hält dann an (Partie pausiert,
Standard) oder überspringt die Aktion (Einstellung). Ein "stilles" Weiterspielen mit abweichendem Zustand macht
den Vergleich wertlos.

## 2. Turmnamen und Koordinaten

**Namen:** dieselbe Zuordnung wie im Konverter (`tools/build_game_v1.py`, `TOWER_NAMES`): `dart` → `DartMonkey`,
`boomerang` → `BoomerangMonkey`, `bomb` → `BombShooter`, `tack` → `TackShooter`, `ice` → `IceMonkey`,
`glue` → `GlueGunner`, `sniper` → `SniperMonkey`, `desperado` → `Desperado`, `super` → `SuperMonkey`,
`druid` → `Druid`, `ninja` → `NinjaMonkey`, `wizard` → `WizardMonkey`. Der Player nimmt das Basis-`TowerModel`
(0-0-0) **aus dem GameModel der laufenden Partie** (`simulation.model`), nicht aus dem Menü-Modell, damit
Schwierigkeit und Modus (Kosten, Tempo) gelten. Die Zuordnung liest der Player aus der Datendatei: `game_v1` hat je
Turm das Feld `game_id` (z.B. `DartMonkey`, Konverter seit Commit 040c8a6), eine eigene Tabelle in der Mod gibt es
nicht.

**Koordinaten:** Replays und `StartState` stehen schon in Spielkoordinaten (Ursprung Mitte, x rechts, y unten,
Spielfeld ±150 × ±116; z.B. `dart` bei (−35, 39) in `data/local/starts/`). Der Player übergibt sie unverändert als
`UnityEngine.Vector2` (so die Signatur von `CreateTowerAt`/`QueryPlacementPosition`). Gemessen (§10): Die Werte sind
Sim-Koordinaten und werden exakt übernommen, auch krumme (−35,37, 59,81); nichts rastet ein.

**Upgrade/Verkauf** adressieren einen Turm über eine Position (Footprint enthält den Punkt). Der Player führt eine
Tabelle Replay-Platzierung → `ObjectId` (aus Schritt 3 oben) und löst Upgrades darüber auf, mit denselben
Footprint-Radien wie die Sim (`game_v1`), nicht mit denen des Spiels: so trifft der Player denselben Turm wie die Sim.
Mehrdeutig oder kein Treffer = Fehler.

## 3. Startzustände mitten im Spiel

`StartState` = Runde, Cash (auch Bruchteile), Leben, Türme mit Stufen (in Bau-Reihenfolge). Drei Wege, von
realistisch nach aufwendig:

**A. Echter Modus, Zustand vor der ersten Runde aufbauen (empfohlen).** Partie im Ziel-Modus starten (z.B.
Monkey Meadow, Hard, CHIMPS); vor dem ersten `StartRound`:
1. Cash hoch setzen, alle Türme mit **echten Preisen** setzen und upgraden (wie `StartState::build` in der Engine:
   so stimmt der investierte Wert für Verkaufspreise); erst weiter, wenn alle Callbacks gefeuert haben und
   `bridge.actions` leer ist (sonst würde ein verspätet abgezogener Preis das gesetzte Geld verfälschen),
2. danach `SetCash(state.cash)`, Leben über `health`/`maxHealth` (CHIMPS: max 1; ein Zustand mit mehr Leben wird
   für CHIMPS vorher abgelehnt),
3. `bridge.SetRound(state.round, false)`, dann `StartRound()`. Zählungen: `StartState.round` = Anzahl **geschaffter**
   Runden (`start.rs`), Replay-Runden zählen ab 1 (CHIMPS ab 6), das Spiel zählt intern ab 0 (intern = angezeigte
   Runde − 1). Die nächste zu spielende Runde ist also `state.round + 1`, intern `state.round` (gemessen:
   `SetRound(9, false)` vor dem ersten Start → Runde 10).

Vorteil: gleiche Regeln wie im Ziel-Modus (Rundenset, Income, Tempo-Faktor, Kostenfaktor). Den Rundenbonus legt das
Spiel je Runde fest (in `game_v1` für CHIMPS z.B. `end_bonus` 101, 102, … je Runde), er hängt also nicht an der
Partie-Historie. Gemessen in CHIMPS (§10): `SetCash` übernimmt auch Bruchteile; `health.Value` lässt sich setzen,
`maxHealth` (1) wird dabei **nicht** erzwungen, der Player hält CHIMPS-Leben deshalb selbst auf 1. Preise beim
Aufbau sind die echten Hard-Preise (Dart 215).

**B. Sandkasten.** Geld, Leben und Runde sind frei setzbar (Spiel-UI dafür), aber andere Regeln (Kosten, Income
gibt es nicht wie im Spiel, Rundenstart anders). Gut für Tempo-/Schuss-Messungen, **nicht** für Partievergleiche.

**C. Spielstand-Datei schreiben.** BTD6 speichert laufende Partien; einen Stand von außen zu erzeugen wäre genau,
aber das Format ist intern und versionsabhängig. Nicht empfohlen.

**D. Headless (später):** Das Spiel hat eine eingebaute Simulation ohne UI:
`SimulationTestUtilities.LoadEnvironmentForTest(SimulationTest, …)` (weitere Parameter; Mod Helper nutzt nur den
ersten) liefert ein `SimulationTestEnvironment` mit `simulation` (Bridge), `isRealTime` und `simulationSpeed`. Mod
Helper nutzt es für automatische Tests (`ModTest.SetupSimEnvironment`). Damit könnten Replays schneller als Echtzeit
und ohne Grafik im echten Spiel-Code laufen: viele Vergleiche hintereinander. **[prüfen]** (noch offen, eigener
Test vor Schritt 5): wie man sie Schritt für Schritt vorantreibt, ob Recorder-Hooks dort feuern, ob Modus und
Schwierigkeit gleich gesetzt werden. Lohnt sich, sobald Weg A steht.

Partie starten kann der Player selbst: Mod Helpers Tests setzen `InGameData.Editable.selectedMap/Mode/Difficulty`
und rufen `UI.instance.LoadGameEnumerator()`. Karte, Modus und Schwierigkeit kommen aus `game_v1.meta`
(Konverter seit Commit 040c8a6): `meta.map` (z.B. `Tutorial`), `meta.mode` (Spiel-Modusname wie `Standard`,
`Clicks`), `meta.difficulty` (`Medium`, `Hard`), `meta.game_version` (z.B. `57.4`); die Namen sind die des Spiels,
wie in den `matches/`-Dateinamen des Dumps. `meta` ist optional: Datendateien ohne `meta` (z.B. `sim_v0`, ältere
`game_v1`) lehnt der Player ab mit dem Hinweis, die Datei mit dem aktuellen Konverter neu zu bauen.

## 4. Zeitpunkte

**Heute (`btd-replay/2`): zwischen den Runden.** Ein Eintrag `r` enthält die Aktionen **vor** Runde `r` und das
Ergebnis **nach** ihr (`replay.rs`). Ablauf je Eintrag: Aktionen von `r` der Reihe nach, jede bestätigt (Abschnitt 1)
→ `StartRound()` → `round_end` abwarten (`AreRoundsActive()` false) → mit `result` von `r` vergleichen (Leben, Cash,
Runde). `result.cash` sind ganze Dollar, im Spiel ist Cash ein Bruch: verglichen wird abgerundet, wie
`Money::dollars` in der Engine (`div_euclid`); der Recorder schreibt zusätzlich den Rohwert. Vorspulen ist erlaubt
(Zeitbasis fest).

**Später (`btd-replay/3`, `docs/design-recorder.md` §1): mit Frame.** Aktion `{"frame": n, …}` = Rundenzeit in
Sim-Schritten. Der Player führt sie in einem Harmony-**Prefix** auf `Simulation.Simulate` aus, sobald
`roundTime.elapsed` den Wert erreicht. Gemessen (§10): auch der Aufruf aus dem Prefix geht in die Warteschlange und
wird **einen Schritt später** wirksam, also vor dem nächsten Sim-Schritt, ob der im selben Frame liegt oder im
nächsten. Die Warteschlange wird vor jedem Sim-Schritt geleert, nicht erst einmal je Frame nach der Schritt-Schleife;
bestätigt an allen drei Positionen im Frame (§10, zweiter Lauf). Der Versatz ist also immer 1, und der Player ruft für
Schritt N im Prefix bei N − 1 auf. Der Recorder protokolliert den tatsächlichen Schritt jeder Aktion, so fällt eine
Abweichung sofort auf. Karte, Modus, Schwierigkeit und Spielversion braucht der `/3`-Header nicht: sie stehen in
`meta` der Datendatei (Abschnitt 3).

**Targeting:** heute immer "First" (wie die Sim); sobald das Replay Targeting kennt, `SetNextTowerTargetType` so oft,
bis der gewünschte Typ eingestellt ist (Kontrolle über den Turm).

## 5. Ein- und Ausgabe, Bedienung, Sicherheit

- **Laden:** Einstellung "Replay-Datei" (Pfad, Unix-Pfade wie beim Dump-Ordner) plus Hotkey **F10** (laden und
  starten) und **F11** (Abbruch). Alternativ legt die Python-Seite die Datei in `data/local/replay_inbox/`
  (gitignored wie `data/local/`) und der Player bietet die neueste an. Unterstützt: `btd-replay/2` (`.jsonl`) und
  `StartState` (`.json`, ein Objekt); ein Replay kann mit einem Startzustand beginnen.
- **Datendatei:** das Replay nennt sie im Header (`data`, z.B. `game_v1_tutorial_chimps.json`); der Player sucht sie
  im Datenordner (Einstellung, Standard `data/local/`) und prüft `data_hash`, falls gesetzt: xxh3_64 mit Seed
  `0xDA7A` über den Dateitext mit CRLF → LF, als 16 Hex-Ziffern klein (wie `engine/crates/btd-env/src/lib.rs`).
- **Prüfen vor dem Start:** Format; Datendatei hat `meta` (Abschnitt 3); `meta.map`/`mode`/`difficulty` gegen die
  laufende Partie (bzw. damit eine neue starten); `meta.game_version` gegen `Application.version`; jeder Turm hat
  eine `game_id`, die es im GameModel der Partie gibt.
  Fehler → nichts tun, Meldung.
- **Ausgabe:** neben dem Trace des Recorders eine Ergebnisdatei
  `data/raw/recordings/<Zeit>_<Karte>_<Modus>.player.jsonl`: je Aktion Status (ok/Fehler + Grund, Sim-Schritt,
  Cash vorher/nachher), je Runde das echte Ergebnis (Leben, Cash, Pops) neben dem erwarteten aus dem Replay. Das
  echte Ergebnis ist selbst wieder ein `btd-replay/2`, damit `btd-tools replay --compare` es direkt liest.
- **Abbruch:** F11, Niederlage, Fehler (Standard: anhalten und pausieren), Verlassen der Partie. Der Recorder wird
  sauber geschlossen.
- **Sicherheit:** nur offline bzw. auf einem Profil, bei dem ein Mod-Flag egal ist (wie bisher). Der Player
  verändert Geld, Leben und Runde direkt, das ist Cheating im Sinne des Spiels: Partien mit Player zählen nicht für
  Erfolge/Bestenlisten, und der Player verweigert den Start in Online-Modi (Koop, Race, Boss-Events, Odyssey,
  Contested Territory; erkennbar an `InGameData.gameType`/`IsCoop`). Er schreibt nie in Spielstände/Profil.
- **Daten:** Replays, Ergebnisse und Traces bleiben lokal (gitignored), wie Dump und Aufnahmen.

## 6. Weg zur Live-Bridge (Stufe 3)

Der Player besteht aus drei Teilen, von denen zwei unverändert in die Live-Bridge übergehen:

1. **Aktions-Ausführer:** Aktion (`place`/`upgrade`/`sell`/`start_round`, optional `frame`) → Bridge-Aufruf mit
   Prüfung und Rückmeldung (Abschnitt 1). Gemeinsam.
2. **Zustands-Leser:** aktueller Stand als `StartState` (Runde, Cash, Leben, Türme mit Position/Stufen) plus
   Rundenergebnis. Gemeinsam; nützt schon vorher, um aus einer echten Partie einen `StartState` zu exportieren
   (Bridge-Fälle aus `docs/design-start-states.md` mit echten Zahlen belegen).
3. **Quelle:** beim Player die Datei, bei der Bridge ein lokaler Socket (Python-Agent schickt die nächste Aktion,
   bekommt Zustand und Ergebnis zurück; gleiche JSON-Zeilen wie im Replay). Nur dieser Teil kommt neu dazu.

Damit ist die Bridge im Kern "Replay, das Zeile für Zeile über den Socket kommt"; der Player ist ihr Testfall.

## 7. Grenzen

- Zufall im Spiel (Crit-Abstände, Streuung, manche Ziele) läuft im Spiel anders als in der Sim; ein exakter
  Gleichlauf ist nur ohne solche Mechaniken zu erwarten, sonst statistischer Vergleich (wie in
  `docs/design-recorder.md` §3).
- Fähigkeiten, Helden, Kräfte und Monkey Knowledge kennt die Sim (noch) nicht; Partien dafür ohne Knowledge spielen
  (CHIMPS schaltet es ab, sonst eigenes Profil oder Modus ohne Knowledge).
- Weg A setzt Geld/Leben/Runde von außen; ob das Spiel dabei intern etwas anders verbucht (z.B. Statistiken,
  "least cash"-Werte), ist für den Vergleich egal, aber ein Grund mehr für ein Profil ohne Wert.

## 8. Reihenfolge

1. **Test-Mod (wie beim Recorder), erledigt (§10):** `CreateTowerAt` mit Sim-Koordinaten, Ausführungsschritt von
   Bridge-Aktionen (auch aus einem `Simulate`-Prefix), `SetRound`-Zählung, `SetCash`/Leben in CHIMPS,
   Callback-Verhalten bei Fehlern, Prefix-Versatz an allen drei Positionen im Frame.
2. **Player für `btd-replay/2` zwischen den Runden** (Weg A, Abschnitt 4), mit Ergebnisdatei; Test: ein Sim-Replay
   aus `data/local/starts/` im Spiel abspielen, `btd-tools replay --compare` auf die Ergebnisse.
3. **Startzustände** (`StartState`) und Export eines `StartState` aus einer echten Partie.
4. **Frames** (`btd-replay/3`), zusammen mit Recorder-Schritt 2 und dem Engine-Umbau für Aktionen mitten in der Runde.
5. **Headless** (Weg D) für viele Vergleiche hintereinander, falls Schritt 1 dafür grünes Licht gibt.
6. **Live-Bridge** (Socket als Quelle).

## 9. Entschieden (2026-10-27)

1. Weg A (echter Modus, Zustand aufbauen) ist Standard; der Sandkasten nur für Messungen.
2. Bei einem Fehler hält der Player an (Partie pausiert), statt die Aktion zu überspringen.
3. Laden per Einstellung "Replay-Datei" plus Hotkey F10 (laden und starten) und F11 (Abbruch); der Eingangsordner
   `data/local/replay_inbox/` kommt als Option dazu. (F10 belegt bis dahin der Player-Test aus Schritt 1.)
4. `game_v1` hat `meta` (Karte, Modus, Schwierigkeit, Spielversion) und `game_id` je Turm (Commit 040c8a6,
   Trainings-Session); der Player liest beides von dort.

## 10. Ergebnis Schritt 1: Player-Test (2026-10-27, BTD6 57.4, Monkey Meadow, CHIMPS)

Test-Mod `mod/BloonsMLMod/Player/PlayerProbe.cs` (F10), Ergebnis und Trace lokal unter `data/raw/recordings/`.
Damit sind die offenen Punkte aus den Abschnitten 1, 2, 3 (Weg A) und 4 beantwortet; offen bleibt Weg D (headless):

- **Ausführung über die Warteschlange, aber vor dem nächsten Sim-Schritt.** `CreateTowerAt` und `UpgradeTower` aus
  `OnUpdate` heraus: Callback nicht während des Aufrufs, `bridge.actions` hat danach 1 Eintrag; der Callback kommt
  im nächsten Frame mit **derselben** Sim-Zeit (z.B. Aufruf bei `t` 592, Callback und `tower_placed` bei `t` 592).
  Zwischen den Runden heißt das: Aktion → einen Frame warten → bestätigt, bevor die Simulation weiterläuft.
- **Aus dem `Simulate`-Prefix: ein Schritt Versatz.** Aufruf im Prefix bei Rundenzeit 300 (`t` 5017): auch hier
  Warteschlange (`queuedActionsAfterCall` 1, `callbackFiredDuringCall` false); Callback im selben Frame, Turm bei
  Rundenzeit **301** (`t` 5018), laut Recorder ebenfalls 301. Vorgespult, 3 Schritte je Frame. Der Recorder schreibt
  `step` im Postfix, der Prefix sieht also die Zeit des vorigen Schritts: der Aufruf (Prefix sieht `t` 5017) gehört
  zum Schritt, dessen Postfix `t` 5018 meldet, dem **2.** Schritt von Frame 6090. Im Trace folgt `tower_placed`
  (`t` 5018) direkt auf den `step` dieses Postfix und kommt vor dem `step` des 3. Schritts (`t` 5019): der Turm
  entstand zwischen dem 2. und 3. Schritt, bevor der 3. die Zeit erhöhte. Würde die Warteschlange
  nur einmal je Frame nach der Schritt-Schleife geleert, stünde der Turm erst bei `t` 5019; die Messung spricht also
  für einen festen Versatz von 1.
- **Bestätigung an allen drei Positionen im Frame** (zweiter Lauf, vorgespult, 3 Schritte je Frame; Player-Test
  Teil 2 setzt bei Rundenzeit 300, 301, 302 je einen Turm und zählt die Position im Frame im Prefix):

  | Aufruf (Rundenzeit) | Position im Frame | Turm laut Callback und Trace |
  |---|---|---|
  | 300 | 2. von 3 | 301, zwischen 2. und 3. Schritt desselben Frames |
  | 301 | 3. von 3 | 302, im nächsten Frame vor dessen 1. Schritt |
  | 302 | 1. von 3 | 303, zwischen 1. und 2. Schritt |

  Jeder Aufruf geht in die Warteschlange (`queuedActionsAfterCall` 1, kein Callback während des Aufrufs); die
  Warteschlange wird vor jedem Sim-Schritt geleert, unabhängig von der Frame-Grenze. Der Versatz ist fest 1.
- **Koordinaten:** `CreateTowerAt` nimmt Sim-Koordinaten (im `UnityEngine.Vector2`); (−35, 39) und (−35,37, 59,81)
  landen exakt (Abweichung 0), nichts rastet ein. `QueryPlacementPosition` gibt die Position unverändert zurück.
- **Fehlerfälle:** belegter Platz und Strecke: `positionValid`/`CanPlaceTowerAt` schon vorher `false`; zu wenig Geld
  für Turm oder Upgrade: Platz gültig, Callback `false`. In allen Fällen kein Turm/keine Stufe und kein Geld abgezogen.
  Prüfung vorher + Callback reichen also für die Fehlererkennung aus Abschnitt 1.
- **Preise (Hard):** Dart 215, Sharp Shots 150 (Menü-Preis × 1,08, auf 5 gerundet); `worth` danach 365 (Summe).
- **Geld:** `SetCash` übernimmt Bruchteile (1234,5) unverändert.
- **Leben:** `health.Value` lässt sich in CHIMPS auf 5 setzen und bleibt so (`maxHealth` 1 wird nicht erzwungen,
  begrenzt wohl nur Effekte, die Leben geben). Der Player muss CHIMPS-Leben selbst auf 1 halten (Abschnitt 3).
- **Runde:** intern = angezeigte Runde − 1 (CHIMPS vor dem Start: `GetCurrentRound()` 5 = Runde 6,
  `spawner.spawnedRound` 4). `bridge.SetRound(9, false)` vor dem ersten Start → nächste Runde intern 9 = **Runde 10**
  (102 Blue, wie im Spiel). Für einen `StartState` mit `round` geschafften Runden: `SetRound(state.round, false)`.
  `spawner.SetRound` (Mod Helper) wurde nicht gebraucht.
- **Nebenbefund für den Recorder:** `Bloon.spawnRound` ist hier für alle Bloons 0, also nicht die Rundennummer;
  `mod/tools/analyze_trace.py` nimmt die Runde für den Bloon-Schlüssel deshalb jetzt aus `round_start`.

Damit kann Schritt 2 (Player für `btd-replay/2` zwischen den Runden, Weg A) ohne offene API-Fragen beginnen.
