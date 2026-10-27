# Design: Replay-Player in der Mod (Entwurf, wartet auf Freigabe)

Ziel: Ein Spiel aus der Sim (Replay `btd-replay/2`, später `/3`) oder ein Startzustand (`StartState`) wird im
echten BTD6 **exakt** ausgeführt: Türme an die exakten Koordinaten, Upgrades und Verkäufe zum richtigen Zeitpunkt,
Runden starten. Der Recorder (`docs/design-recorder.md`) läuft dabei mit. So lässt sich ein Sim-Spiel 1:1 im Spiel
abspielen und Tick für Tick vergleichen, statt aus Screenshots oder Endständen zurückzurechnen.

Baut auf der vorhandenen Mod auf (`mod/BloonsMLMod`): neuer Ordner `Player/`, gemeinsame Teile mit dem Recorder in
`Common/`. Stand der Prüfung: BTD6 57.4, Mod Helper 3.6.10. Signaturen unten stammen aus den generierten
Il2Cpp-Assemblies; was nur aus Signaturen gefolgert ist, ist als **[prüfen]** markiert und kommt in die Test-Mod
(Abschnitt 8, Schritt 1).

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

**Ausführungszeitpunkt:** Im Normalfall laufen `CreateTowerAt` und `UpgradeTower` **sofort** (Mod Helpers Tests lesen
`GetAllTowers()` bzw. das Callback-Ergebnis direkt nach dem Aufruf); nur in manchen Zuständen landen sie in der
Warteschlange `bridge.actions` (Mod Helper warnt dann "could not be immediately performed"). `SetCash`, `SetRound`
und `ResetRound` haben keine `_Impl`-Variante, laufen also nie über die Warteschlange. Der Player wartet deshalb nach
jeder Aktion, bis ihr Callback gefeuert hat **und** `bridge.actions` leer ist, bevor er weitermacht. **[prüfen]**:
wann genau Aktionen in die Warteschlange gehen und welche Methode sie leert (`UnityToSimulation.Simulate` einmal je
Unity-Frame oder je Sim-Schritt?), wichtig für Aktionen mit Frame-Angabe (Abschnitt 4).

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
Schwierigkeit und Modus (Kosten, Tempo) gelten. Damit die Liste nicht zweimal gepflegt wird: der Konverter schreibt
sie zusätzlich in `game_v1` (Feld `game_id` je Turm), der Player liest sie von dort; bis dahin eine feste Tabelle
in der Mod, gegen die ein Test die Konverter-Liste prüft.

**Koordinaten:** Replays und `StartState` stehen schon in Spielkoordinaten (Ursprung Mitte, x rechts, y unten,
Spielfeld ±150 × ±116; z.B. `dart` bei (−35, 39) in `data/local/starts/`). Der Player übergibt sie unverändert als
`UnityEngine.Vector2` (so die Signatur von `CreateTowerAt`/`QueryPlacementPosition`). **[prüfen]**: ob die Werte
dort Sim-Koordinaten sind (Mod Helpers eigener Test setzt bei (0, 0), also Spielfeldmitte, was dafür spricht) und ob
das Spiel die Position einrastet; der Test in Schritt 1 setzt Türme an krumme Koordinaten und vergleicht.

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
3. Runde setzen, dann `StartRound()`. Zählungen: `StartState.round` = Anzahl **geschaffter** Runden (`start.rs`),
   Replay-Runden zählen ab 1 (CHIMPS ab 6), das Spiel zählt intern ab 0 (der Recorder zeigt `GetCurrentRound()` 0
   für Runde 1). Die nächste zu spielende Runde ist also `state.round + 1`, intern `state.round`.

Vorteil: gleiche Regeln wie im Ziel-Modus (Rundenset, Income, Tempo-Faktor, Kostenfaktor). Den Rundenbonus legt das
Spiel je Runde fest (in `game_v1` für CHIMPS z.B. `end_bonus` 101, 102, … je Runde), er hängt also nicht an der
Partie-Historie. **[prüfen]**:
- ob `bridge.SetRound(round, false)` oder Mod Helpers `spawner.SetRound(round)` vor der ersten Runde greift und mit
  welcher Zählung (siehe oben);
- ob CHIMPS Geld- und Leben-Setzen zulässt (CHIMPS sperrt u.a. Verkaufen, Continue, Knowledge, Extra-Leben und
  Kräfte; `SetCash` ist laut Signatur ein Bridge-Aufruf ohne Modus-Prüfung).

**B. Sandkasten.** Geld, Leben und Runde sind frei setzbar (Spiel-UI dafür), aber andere Regeln (Kosten, Income
gibt es nicht wie im Spiel, Rundenstart anders). Gut für Tempo-/Schuss-Messungen, **nicht** für Partievergleiche.

**C. Spielstand-Datei schreiben.** BTD6 speichert laufende Partien; einen Stand von außen zu erzeugen wäre genau,
aber das Format ist intern und versionsabhängig. Nicht empfohlen.

**D. Headless (später):** Das Spiel hat eine eingebaute Simulation ohne UI (`SimulationTestUtilities.
LoadEnvironmentForTest(SimulationTest, …)` (weitere Parameter, Mod Helper nutzt nur den ersten) → `SimulationTestEnvironment` mit `simulation` (Bridge), `isRealTime`,
`simulationSpeed`), die Mod Helper für automatische Tests nutzt (`ModTest.SetupSimEnvironment`). Damit könnten
Replays schneller als Echtzeit und ohne Grafik im echten Spiel-Code laufen: viele Vergleiche hintereinander.
**[prüfen]**: wie man sie Schritt für Schritt vorantreibt, ob Recorder-Hooks dort feuern, ob Modus/Schwierigkeit
gleich gesetzt werden. Lohnt sich, sobald Weg A steht.

Partie starten kann der Player selbst: Mod Helpers Tests setzen `InGameData.Editable.selectedMap/Mode/Difficulty`
und rufen `UI.instance.LoadGameEnumerator()`. Karte/Modus/Schwierigkeit stehen in `game_v1` heute nicht als eigene
Felder (nur `name`, z.B. `Tutorial-chimps-57.4`, und `map.name`); bis der Konverter sie und die Spielversion als Felder
schreibt (oder der `/3`-Header sie trägt, Abschnitt 4), kommen sie **aus Einstellungen des Players** (Karte, Modus,
Schwierigkeit). Aus `name` lassen sie sich nicht verlässlich ablesen: `Tutorial-medium60-57.4` nennt ein Preset statt
einer Schwierigkeit, `Tutorial-chimps-57.4` nur den Modus (Hard ist implizit), und `sim_v0` enthält gar nichts davon.
Der Player prüft aber die Karte (`map.name`) und, wo vorhanden, die Version am Ende von `name` gegen das Spiel.

## 4. Zeitpunkte

**Heute (`btd-replay/2`): zwischen den Runden.** Ein Eintrag `r` enthält die Aktionen **vor** Runde `r` und das
Ergebnis **nach** ihr (`replay.rs`). Ablauf je Eintrag: Aktionen von `r` der Reihe nach, jede bestätigt (Abschnitt 1)
→ `StartRound()` → `round_end` abwarten (`AreRoundsActive()` false) → mit `result` von `r` vergleichen (Leben, Cash,
Runde). `result.cash` sind ganze Dollar, im Spiel ist Cash ein Bruch: verglichen wird abgerundet, wie
`Money::dollars` in der Engine (`div_euclid`); der Recorder schreibt zusätzlich den Rohwert. Vorspulen ist erlaubt
(Zeitbasis fest).

**Später (`btd-replay/3`, `docs/design-recorder.md` §1): mit Frame.** Aktion `{"frame": n, …}` = Rundenzeit in
Sim-Schritten. Der Player führt sie in einem Harmony-**Prefix** auf `Simulation.Simulate` aus, sobald
`roundTime.elapsed` den Wert erreicht. Ziel ist, dass sie sofort im selben Schritt läuft und nicht in der
Warteschlange landet: die wird womöglich nur einmal je Unity-Frame geleert, und vorgespult laufen 3 Sim-Schritte je
Frame (Aktion könnte bis zu 2 Schritte zu spät kommen). **[prüfen]**: ob ein Aufruf aus dem `Simulate`-Prefix sofort
läuft oder gerade dann in die Warteschlange geht; sonst die Simulation direkt statt über die Bridge ansprechen. Der
Recorder protokolliert den tatsächlichen Schritt jeder Aktion, so fällt eine Abweichung sofort auf. Der Header von `/3` bekommt zusätzlich
`map`, `mode`, `difficulty`, `game_version` (der Player bricht ab, wenn die laufende Partie nicht passt).

**Targeting:** heute immer "First" (wie die Sim); sobald das Replay Targeting kennt, `SetNextTowerTargetType` so oft,
bis der gewünschte Typ eingestellt ist (Kontrolle über den Turm).

## 5. Ein- und Ausgabe, Bedienung, Sicherheit

- **Laden:** Einstellung "Replay-Datei" (Pfad, Unix-Pfade wie beim Dump-Ordner) plus Hotkey (Vorschlag **F10**:
  laden und starten; **F11**: Abbruch). Alternativ legt die Python-Seite die Datei in
  `data/local/replay_inbox/` (gitignored wie `data/local/`) und der Player bietet die neueste an. Unterstützt: `btd-replay/2` (`.jsonl`) und
  `StartState` (`.json`, ein Objekt); ein Replay kann mit einem Startzustand beginnen.
- **Datendatei:** das Replay nennt sie im Header (`data`, z.B. `game_v1_tutorial_chimps.json`); der Player sucht sie
  im Datenordner (Einstellung, Standard `data/local/`) und prüft `data_hash`, falls gesetzt: xxh3_64 mit Seed
  `0xDA7A` über den Dateitext mit CRLF → LF, als 16 Hex-Ziffern klein (wie `engine/crates/btd-env/src/lib.rs`).
- **Prüfen vor dem Start:** Format; Karte (`map.name` der Datendatei) gegen die Partie; Modus/Schwierigkeit kommen
  aus den Einstellungen und werden bei einer schon laufenden Partie gegen diese geprüft; Spielversion gegen die
  Version im Namen der Datendatei, sofern vorhanden (z.B. `…-57.4`, siehe Abschnitt 3); alle Turmnamen bekannt.
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

1. **Test-Mod (wie beim Recorder):** im Spiel prüfen, was oben **[prüfen]** ist: `CreateTowerAt` mit Sim-Koordinaten
   (Einrasten? Koordinatensystem des `UnityEngine.Vector2`?), Ausführungsschritt von Bridge-Aktionen (auch aus einem `Simulate`-Prefix heraus), `SetRound`-Zählung vor der ersten Runde,
   `SetCash`/Leben in CHIMPS, Callback-Verhalten bei Fehlern (zu wenig Geld, belegter Platz).
2. **Player für `btd-replay/2` zwischen den Runden** (Weg A, Abschnitt 4), mit Ergebnisdatei; Test: ein Sim-Replay
   aus `data/local/starts/` im Spiel abspielen, `btd-tools replay --compare` auf die Ergebnisse.
3. **Startzustände** (`StartState`) und Export eines `StartState` aus einer echten Partie.
4. **Frames** (`btd-replay/3`), zusammen mit Recorder-Schritt 2 und dem Engine-Umbau für Aktionen mitten in der Runde.
5. **Headless** (Weg D) für viele Vergleiche hintereinander, falls Schritt 1 dafür grünes Licht gibt.
6. **Live-Bridge** (Socket als Quelle).

## 9. Zu entscheiden

1. Weg A (echter Modus, Zustand aufbauen) als Standard, Sandkasten nur für Messungen?
2. Bei einem Fehler anhalten (Standard) oder überspringen?
3. Laden per Einstellung + Hotkey (F10/F11) oder über einen Eingangsordner?
4. Konverter-Änderung in der Trainings-Session: `game_v1` bekommt `game_id` je Turm sowie Karte, Modus,
   Schwierigkeit und Spielversion als eigene Felder (statt Tabelle in der Mod bzw. Einstellungen im Player)?
