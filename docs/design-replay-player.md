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
| Geld | `SetCash(double value, inputId)`, `GetCash()`; Mod Helper `InGame.SetCash` setzt `cashManager.cash.Value` |
| Leben | `simulation.health.Value` / `maxHealth.Value` (Mod Helper `SetHealth`, `SetMaxHealth`); Sandkasten: `SetSandboxHealth(float)` |
| Kosten | `GetTowerCost(tm, inputId)`, `GetUpgradeCost(id, path, tier, …)`, `IsUpgradeLocked(id, path, tier)` |
| Vorspulen | `SetFastForward(bool)` (Zeitbasis ist fest, `docs/design-recorder.md` §6: Vorspulen ändert nichts am Ablauf) |
| Targeting | `SetNextTowerTargetType(inputId, id, prev)` (schaltet eine Option weiter) |
| Türme lesen | `GetAllTowers()` → `TowerToSimulation`, `GetTowerFromId(id)` |

**Ausführungszeitpunkt:** Bridge-Aufrufe landen in einer Warteschlange (`bridge.actions`) und werden beim nächsten
Sim-Schritt ausgeführt (Mod Helper warnt in seinen Tests, wenn `CreateTowerAt` nicht sofort durchging). Das ist für
uns gut: jede Aktion fällt auf einen definierten Sim-Schritt, den der Recorder als `t`/`rt` mitschreibt. **[prüfen]**:
auf welchen Schritt genau (gleicher oder nächster), damit Aktionen mit Frame-Angabe (Abschnitt 4) passen.

**Wie Mod Helper es selbst macht:** `Api/Testing/ModTest.CreateTowerAt` ruft `CreateTowerAt` mit
`ignoreInventoryChecks`/`ignorePlacementChecks` auf und nimmt den neuen Turm als Differenz von `GetAllTowers()` vorher
und nachher; `ignoreInventoryChecks` greift laut Mod Helper nicht, dafür setzt es einen eigenen Patch
(`TowerInventory_HasInventory.overrideValue`). Wir rufen ohne `ignore…` auf, damit das Spiel genau so prüft wie bei
einem Spieler.

**Platzierungsfehler erkennen (vor, bei und nach der Aktion):**

1. Vorher: `QueryPlacementPosition` → `positionValid`; dazu Kosten gegen `GetCash()`.
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
Tabelle Replay-Platzierung → `ObjectId` (aus Schritt 3 oben) und löst Upgrades zuerst darüber auf, nur ersatzweise
über den Footprint; mehrdeutig oder kein Treffer = Fehler.

## 3. Startzustände mitten im Spiel

`StartState` = Runde, Cash (auch Bruchteile), Leben, Türme mit Stufen (in Bau-Reihenfolge). Drei Wege, von
realistisch nach aufwendig:

**A. Echter Modus, Zustand vor der ersten Runde aufbauen (empfohlen).** Partie im Ziel-Modus starten (z.B.
Monkey Meadow, Hard, CHIMPS); vor dem ersten `StartRound`:
1. Cash hoch setzen, alle Türme mit **echten Preisen** setzen und upgraden (wie `StartState::build` in der Engine:
   so stimmt der investierte Wert für Verkaufspreise),
2. danach `SetCash(state.cash)`, Leben über `health`/`maxHealth` (CHIMPS: max 1),
3. `SetRound(state.round, false)`, dann `StartRound()`.

Vorteil: gleiche Regeln wie im Ziel-Modus (Rundenset, Income, Tempo-Faktor, Kostenfaktor). **[prüfen]**:
- ob `SetRound` vor der ersten Runde greift und welche Zählung es nutzt (0- oder 1-basiert; der Recorder zeigt
  `GetCurrentRound()` 0 für Runde 1);
- ob CHIMPS Geld-Setzen zulässt (CHIMPS sperrt nur Verkaufen/Continue/Knowledge; `SetCash` ist ein Bridge-Aufruf
  ohne Modus-Prüfung, laut Signatur);
- ob Income/Rundenbonus für die gesetzte Runde stimmt (Bonus hängt an der Runde, nicht an der Partie-Historie).

**B. Sandkasten.** Geld, Leben und Runde sind frei setzbar (Spiel-UI dafür), aber andere Regeln (Kosten, Income
gibt es nicht wie im Spiel, Rundenstart anders). Gut für Tempo-/Schuss-Messungen, **nicht** für Partievergleiche.

**C. Spielstand-Datei schreiben.** BTD6 speichert laufende Partien; einen Stand von außen zu erzeugen wäre genau,
aber das Format ist intern und versionsabhängig. Nicht empfohlen.

**D. Headless (später):** Das Spiel hat eine eingebaute Simulation ohne UI (`SimulationTestUtilities.
LoadEnvironmentForTest(SimulationTest)` → `SimulationTestEnvironment` mit `simulation` (Bridge), `isRealTime`,
`simulationSpeed`), die Mod Helper für automatische Tests nutzt (`ModTest.SetupSimEnvironment`). Damit könnten
Replays schneller als Echtzeit und ohne Grafik im echten Spiel-Code laufen: viele Vergleiche hintereinander.
**[prüfen]**: wie man sie Schritt für Schritt vorantreibt, ob Recorder-Hooks dort feuern, ob Modus/Schwierigkeit
gleich gesetzt werden. Lohnt sich, sobald Weg A steht.

Partie starten kann der Player selbst: Mod Helpers Tests setzen `InGameData.Editable.selectedMap/Mode/Difficulty`
und rufen `UI.instance.LoadGameEnumerator()`. Karte/Modus/Schwierigkeit kommen aus den Regeln der `game_v1`-Datei
(`rules.start_round`, `start_lives` …, Name z.B. `Tutorial-chimps-57.4`) bzw. künftig direkt aus dem Replay-Header
(Abschnitt 4).

## 4. Zeitpunkte

**Heute (`btd-replay/2`): zwischen den Runden.** Ablauf je Eintrag:
`round_end` abwarten (`AreRoundsActive()` false) → alle Aktionen der Runde der Reihe nach, jede bestätigt
(Abschnitt 1) → Runden-Ergebnis vergleichen (Leben, Cash, Runde aus dem `result` des Replays) → `StartRound()`.
Vorspulen ist erlaubt (Zeitbasis fest).

**Später (`btd-replay/3`, `docs/design-recorder.md` §1): mit Frame.** Aktion `{"frame": n, …}` = Rundenzeit in
Sim-Schritten. Der Player stellt sie in einem Harmony-**Prefix** auf `Simulation.Simulate` in die Bridge, sobald
`roundTime.elapsed` den Wert erreicht (bzw. einen Schritt früher, je nach Ergebnis von **[prüfen]** in Abschnitt 1).
Damit ist jede Aktion auf den Schritt genau, unabhängig vom Vorspulen. Der Header von `/3` bekommt zusätzlich
`map`, `mode`, `difficulty`, `game_version` (der Player bricht ab, wenn die laufende Partie nicht passt).

**Targeting:** heute immer "First" (wie die Sim); sobald das Replay Targeting kennt, `SetNextTowerTargetType` so oft,
bis der gewünschte Typ eingestellt ist (Kontrolle über den Turm).

## 5. Ein- und Ausgabe, Bedienung, Sicherheit

- **Laden:** Einstellung "Replay-Datei" (Pfad, Unix-Pfade wie beim Dump-Ordner) plus Hotkey (Vorschlag **F10**:
  laden und starten; **F11**: Abbruch). Alternativ legt die Python-Seite die Datei in
  `data/raw/replay_inbox/` und der Player bietet die neueste an. Unterstützt: `btd-replay/2` (`.jsonl`) und
  `StartState` (`.json`, ein Objekt); ein Replay kann mit einem Startzustand beginnen.
- **Prüfen vor dem Start:** Format, Karte/Modus/Schwierigkeit gegen die laufende bzw. zu startende Partie,
  Spielversion gegen `game_version` der Datendatei, alle Turmnamen bekannt. Fehler → nichts tun, Meldung.
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
   (Einrasten? Koordinatensystem des `UnityEngine.Vector2`?), Ausführungsschritt von Bridge-Aktionen, `SetRound`-Zählung vor der ersten Runde,
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
4. Turm-Zuordnung über ein neues Feld `game_id` in `game_v1` (Konverter-Änderung in der Trainings-Session)?
