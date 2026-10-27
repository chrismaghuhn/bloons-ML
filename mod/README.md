# BloonsML-Mod (MelonLoader + BTD6 Mod Helper)

Mod für Bloons TD 6, die die Spieldaten direkt aus den Models des Spiels liest.

- **Stufe 1 (jetzt): Daten-Dump.** Türme mit *allen* gültigen Upgrade-Kombinationen (inkl. Crosspaths wie 2-0-3,
  Helden-Level, Paragons, Sub-Towers), Upgrades, Bloons, Runden-Sets mit Spawn-Timing, Karten-Geometrie und
  Spielversion, als JSON nach `data/raw/game_dump/<version>/`. Das ist eine weitere Rohdatenquelle neben
  `data/raw/towers/` (Wiki), siehe Schichtenmodell in `docs/engine-design.md` §4. Welche Rolle sie dort bekommt und
  ob daraus abgeleitete Basisdaten ins (öffentliche) Repo dürfen, ist noch nicht entschieden.
- Stufe 2 (später): Recorder, der echte Partien aufzeichnet (Platzierungen mit exakten Koordinaten, Upgrades,
  Leben/Geld pro Runde) und Replays für die Kalibrierung schreibt → `Recorder/`.
- Stufe 3 (später): Live-Bridge, über die der Python-Agent per lokalem Socket das echte Spiel steuert → `Bridge/`.

Gemeinsamer Code (Pfade, JSON) liegt in `BloonsMLMod/Common/`, der Dump in `BloonsMLMod/Dump/`.

## Wichtig

- **Gemoddet nur offline / auf einem Profil spielen, bei dem ein Flag egal ist.** Ninja Kiwi kann Profile mit Mods
  markieren (Bestenlisten, Events). Keine Races, Bosse, kein Koop/Wettkampf mit geladener Mod.
- **Der Dump ist Material von Ninja Kiwi und wird nicht committet** (das Repo ist öffentlich).
  `data/raw/game_dump/` steht in `.gitignore`. Ebenso werden keine Spiel- oder Loader-DLLs committet
  (`mod/.gitignore`); die Mod referenziert sie nur über einen lokal konfigurierten Pfad.

## Einrichtung (Linux, Steam + Proton)

Getestet mit BTD6 **57.4** (Windows-Build über Proton, Unity 6000.3), MelonLoader **0.7.2** (von Mod Helper empfohlen)
und BTD6 Mod Helper **3.6.10** (für BTD6 v57.2+): kompletter Dump in ca. 35 s, ohne Fehler.

1. **.NET SDK** (zum Bauen der Mod; das Ziel ist `net6.0`, ein neueres SDK reicht):
   ```bash
   sudo apt install dotnet-sdk-8.0
   ```
2. **MelonLoader 0.7.2** ins Spielverzeichnis entpacken (Steam → BTD6 → Verwalten → Lokale Dateien durchsuchen;
   Standard: `~/.steam/steam/steamapps/common/BloonsTD6`). Die Windows-Variante nehmen, das Spiel läuft unter Proton:
   ```bash
   cd ~/.steam/steam/steamapps/common/BloonsTD6
   curl -LO https://github.com/LavaGang/MelonLoader/releases/download/v0.7.2/MelonLoader.x64.zip
   unzip MelonLoader.x64.zip && rm MelonLoader.x64.zip
   ```
   Danach liegen dort `version.dll` und der Ordner `MelonLoader/`.
   (Alternativ der grafische `MelonLoader.Installer.Linux` aus demselben Release, Version 0.7.2 wählen.)
3. **Steam-Startoption** setzen (BTD6 → Eigenschaften → Allgemein → Startoptionen), damit Wine die
   `version.dll` von MelonLoader lädt:
   ```
   WINEDLLOVERRIDES="version=n,b" %command%
   ```
4. **Einmal starten** und warten, bis das Hauptmenü erscheint. Der erste Start dauert mehrere Minuten
   (MelonLoader lädt Abhängigkeiten herunter und erzeugt `MelonLoader/Il2CppAssemblies/`). Danach Spiel beenden.
   Log: `MelonLoader/Latest.log`.
   Falls das Log über eine fehlende .NET-Runtime / `hostfxr` klagt:
   `protontricks 960090 dotnetdesktop6` und erneut starten.
5. **Mod Helper**: `Btd6ModHelper.dll` aus
   [Release 3.6.10](https://github.com/gurrenm3/BTD-Mod-Helper/releases/tag/3.6.10) nach `BloonsTD6/Mods/` legen
   (nicht umbenennen). Spiel starten: unten rechts im Hauptmenü erscheint ein „Mods“-Knopf.

## Bauen

```bash
cp mod/local.props.example mod/local.props   # Pfade eintragen (gitignored)
dotnet build mod/BloonsMLMod -c Release
```

`local.props` setzt `BloonsTD6` (Spielverzeichnis) und `DumpRoot` (Zielordner, Unix-Pfad; am besten der
Haupt-Checkout `…/bloons/data/raw/game_dump`, nicht ein Worktree). Beide lassen sich auch per
`-p:BloonsTD6=…` übergeben. Ohne `local.props` gelten `~/.steam/steam/steamapps/common/BloonsTD6` und
`data/raw/game_dump` relativ zu diesem Checkout. Der Build kopiert `BloonsMLMod.dll` nach `BloonsTD6/Mods/`
(abschalten mit `-p:CopyToMods=false`). Am besten bei geschlossenem Spiel bauen; die alte DLL wird vor dem
Kopieren gelöscht, die neue erst beim nächsten Start geladen.

`DumpRoot` wird beim Build als Standardwert in die DLL eingebacken. Hat man den Dump-Ordner im Spiel einmal in den
Mod-Einstellungen geändert, speichert Mod Helper diesen Wert und er gilt weiter; eine spätere Änderung von `DumpRoot`
in `local.props` wirkt dann nicht mehr (Einstellung im Spiel zurücksetzen).

Referenziert werden `MelonLoader/net6/*` (MelonLoader, Harmony, Il2CppInterop, Newtonsoft.Json),
`MelonLoader/Il2CppAssemblies/*.dll` und `Mods/Btd6ModHelper.dll`, alles mit `Private=false`.

## Benutzung

| Aktion | Wie |
|---|---|
| Kompletter Dump | **F6** (am besten im Hauptmenü) oder Mods → BloonsML → „Dump“ |
| Karte + Partie-Regeln | automatisch beim Start jeder Partie (Karte nur, wenn sie fehlt oder bisher nur aus einer Reverse-Partie stammt); **F7** schreibt beides erneut |
| Einstellungen | Mods → BloonsML: Dump-Ordner, Hotkeys, Auto-Karten-Dump |

Fortschritt und Fehler stehen im MelonLoader-Log (`BloonsTD6/MelonLoader/Latest.log`, Präfix `[BloonsML]`).
Der Dump dauert etwa 35 s (57.4), das Spiel bleibt dabei bedienbar, ruckelt aber.

Für **alle Karten** muss man jede Karte einmal starten (Geometrie wird erst beim Laden einer Partie erzeugt), für die
Regeln eines Modus (z.B. CHIMPS) eine Partie in diesem Modus. Für das Projekt reicht vorerst Monkey Meadow, intern
`Tutorial`.

Ein kompletter Dump schreibt erst in `<Spielversion>.tmp-<runId>/` und ersetzt `<Spielversion>/` am Ende in einem
Schritt; `maps/` und `matches/` aus dem alten Stand werden übernommen. Bricht der Dump ab (Fehler, Absturz), bleibt der
alte Stand unverändert; der Rest wird beim nächsten Dump aufgeräumt. `manifest.json` trägt die `runId`.

Stürzt das Spiel beim Serialisieren eines `GameModel`- oder `GameData`-Felds ab, merkt sich die Mod das Feld (z.B.
`gameData.questData` in `<DumpRoot>/.bloonsml-skip-fields`) und überspringt es bei allen weiteren Dumps, auch nach einem
Spiel-Update; im Manifest steht es dann unter `errors`. Zum erneuten Versuch die Zeile aus der Datei löschen.

## Test-Recorder (Stufe 2, Schritt 1)

Für die Kalibrierung Sim ↔ Spiel (`docs/design-recorder.md`): Einstellung **„Trace aufzeichnen“** oder Hotkey **F9**
schaltet einen Trace an. Ab dem nächsten Sim-Schritt schreibt die Mod nach
`<DumpRoot>/../recordings/<Zeit>_<Karte>_<Modus>_<Schwierigkeit>.trace.jsonl.gz` (also `data/raw/recordings/`,
gitignored, mit eigener `.gitignore`); die Datei wird bei Partie-Ende, Neustart, Hauptmenü oder Ausschalten
geschlossen. Geschrieben wird in einem Hintergrundthread.

Format `bloonsml-trace/0`, eine JSON-Zeile je Ereignis, `t` = interner Sim-Zähler (`Simulation.time.elapsed`),
`rt` = Rundenzeit (`roundTime.elapsed`):

| `ev` | Felder |
|---|---|
| `header` | Karte, Modus, Schwierigkeit, Spiel-/Mod-Version, `paths[]` mit `spawnDist`, `leakDist`, `maximumDistanceUntilSpawn/Leak`, `mapWideBloonSpeed` |
| `step` | je Sim-Schritt: `t`, `rt`, Unity-Frame `uf`, Schritt-Nr. im Frame `sif`, `timeScale` `ts`, `deltaTime` `dt`, Vorspulen `ff`; alle N Schritte `b` = `[[id, Pfad, Distanz, x, y], …]` |
| `spawn` | beim Erzeugen (`Bloon.Initialise`): `id`, `type`, `spawnRound`, `emissionIndex` (57.4: immer 0), `createdAtRoundTime`, `lineage`; `dist`/`x`/`y`/`health` sind hier noch 0 |
| `bloon_info` | beim ersten Schritt eines Bloons (beim Erzeugen sind Distanz/Position/Leben noch 0): Distanz, Position, `health`, `trackSpeedMultiplier`, `totalSpeedFrames`, `modelSpeed`, `modelSpeedFrames`, `radius` |
| `leak` / `leak_done` | vor/nach dem Leak: Bloon, `health`, `leakDamage`, `lives` |
| `round_start` / `round_end`, `fastforward` | Runde bzw. an/aus |
| `tower_placed` / `tower_upgraded` / `tower_sold` | Turm-ID, Name, Position, Range, Upgrade bzw. Verkaufserlös |
| `shot` | Turm-ID, Waffe, `rateFrames` |

Auswertung: `.venv/bin/python mod/tools/analyze_trace.py TRACE [TRACE2]` (mit zwei Dateien: Vergleich je Bloon
über Runde, Emission-Index und Abstammung zur selben Rundenzeit).

## Player-Test (Replay-Player, Schritt 1)

Test für `docs/design-replay-player.md`: in einer **Wegwerf-Partie** (Monkey Meadow, CHIMPS) vor dem ersten
Rundenstart **F10** drücken. Der Test setzt Dart Monkeys an exakte und krumme Koordinaten, probiert Fehlerfälle
(belegt, auf der Strecke, zu wenig Geld), ein Upgrade, setzt Geld, Leben und Runde und schreibt die Ergebnisse nach
`data/raw/recordings/<Zeit>_probe.json` (Trace wird automatisch eingeschaltet). Danach die Runde starten: bei
Rundenschritt 300 setzt er aus dem `Simulate`-Prefix einen weiteren Turm (Teil 2, ergänzt dieselbe Datei).

## Ausgabeformat

`<DumpRoot>/<Spielversion>/` (Spielversion = `Application.version`, z.B. `57.4`):

| Pfad | Inhalt |
|---|---|
| `manifest.json` | `runId`, Spiel-/Unity-/Mod-Version, Zeitpunkt, Dauer, Anzahl Dateien je Kategorie, Fehlerliste |
| `summary.json` | Kleine Übersicht (siehe unten) |
| `towers/<baseId>/<name>.json` | Ein `TowerModel` je gültiger Kombination, z.B. `towers/DartMonkey/DartMonkey-203.json`; Helden als `Quincy 1…20`, Paragons, Sub-Towers |
| `upgrades/<name>.json` | `UpgradeModel` (Kosten, `path` und `tier` ab 0 gezählt, XP); Namen sind die internen des Spiels (z.B. `Buccaneer-Faster Shooting`, `Large Calibre`) |
| `bloons/<baseId>/<name>.json` | `BloonModel` je Variante (z.B. `Red`, `RedCamo`, `CeramicRegrowFortified`, `Moab`, …) |
| `rounds/<RoundSet>/_set.json` | `name`, `linkedIncomeSet` (→ `gamedata/incomeSets.json`), `roundCount` |
| `rounds/<RoundSet>/NNN.json` | `RoundModel` der Runde NNN (1-basiert): `groups[]` mit `bloon`, `start`, `end`, `count`, ggf. `emissions` |
| `maps/_index.json` | Metadaten aller Karten (`id`, `difficulty`, …) aus `GameData.mapSet` |
| `maps/<mapName>.json` | `{gameVersion, mapName, dumpedInMode, dumpedInDifficulty, reverseMode, map: MapModel}`: Pfade (`paths[].points[].point`), Flächen (`areas[]` mit `polygon` und `type`), Blocker, Spawner |
| `matches/<Spieltyp>_<Modus>_<Schwierigkeit>_<Karte>.json` | Regeln einer echten Partie, geschrieben nachdem das Spiel Modus-, Schwierigkeits- und Challenge-Regeln angewendet hat: `fields` (Partie-Felder des GameModels wie `cash`, `startingHealth`, `maxHealth`, `startRound`/`endRound`, `roundSet`, `incomeSet`, `sellMultiplier`, `gameMode`, `difficultyId`, globale Skalierungen) und `mods` (die `ModModel`s der Partie: Schwierigkeit/Modus und, außer in Modi ohne Knowledge wie CHIMPS, das Monkey Knowledge des Profils, z.B. `MoreCash` = +200 Startgeld). `gameMode`/`difficultyId` sind im GameModel leer; Modus und Schwierigkeit stehen oben im Dokument (aus der Menü-Auswahl). CHIMPS hat die Modus-ID `Clicks` |
| `gamedata/<feld>.json` | Die übrigen Felder von `GameData` einzeln, u.a. `incomeSets` (Rundengeld-Faktoren je Runde), `mods` (Schwierigkeits- und Modus-Presets wie `Clicks` = CHIMPS, `Hard`, `Impoppable`), `modesData`, Bosse, Artefakte; auch reine Asset-/UI-Felder. Nicht enthalten: `questData`, `rogueData`, `frontierData`, `legendsData` (Kampagnen/Events; `questData` lässt BTD6 57.4 beim Serialisieren abstürzen) |
| `gamemodel/<feld>.json` | Statische Felder von `GameModel` einzeln (z.B. `freeplayGroups`, Powers, Wissensbaum, Buff-Indikatoren, Helden-/Turm-Sets, globale Skalierungen). Ohne Duplikate (`upgradesByName`, `bloonsByName`, Backing-Fields) und ohne Partie-Felder, die im Menü nur Standardwerte haben (stehen in `matches/`) |

**Rohformat:** Jede Datei ist eine möglichst unveränderte Serialisierung des Spiel-Models über den Serializer von
Mod Helper (`Il2CppJsonConvert`). Jedes Objekt trägt `"$type"` mit Typname und Assembly, z.B.
`Il2CppAssets.Scripts.Models.Towers.Projectiles.Behaviors.DamageModel, Assembly-CSharp` (zum Vergleichen am Komma
trennen). Ein Turm enthält also seine komplette
Behavior-Hierarchie: `AttackModel` → `weapons[]` (`WeaponModel`, `rate` = Cooldown in s, `emission` z.B.
`ParallelEmissionModel` mit `count`) → `projectile` (`ProjectileModel`, `pierce`, `radius`) → `behaviors[]`
(`DamageModel` mit `damage` und `immuneBloonProperties`, `TravelStraitModel` mit `speed`/`lifespan`,
`CreateProjectileOnContactModel` für Explosionen,
`DamageModifierForTagModel` für Bonusschaden gegen MOAB/Keramik/…, Filter für Camo usw.). Reichweite: `range`
am Turm und an den Attacks; Camo-Erkennung über `FilterInvisibleModel`/Detection-Behaviors.
Feldnamen entsprechen den Feldern im Spiel und können sich mit Updates ändern.

Der Serializer gibt viele Werte doppelt aus, als Feld und als Getter (`rate`/`Rate`, `speed`/`Speed`,
`pierce`/`CappedPierce`, …). **Kanonisch ist das kleingeschriebene Feld**; die Getter können abgeleitet oder gedeckelt
sein. `immuneBloonProperties` ist ein Bitfeld (`BloonProperties`: 1 = Lead, 2 = Black, 4 = White, 8 = Purple,
16 = Frozen, 32 = Immune, 64 = Glass; z.B. 17 = Lead + Frozen für spitze Projektile).

Einheiten: Distanzen in Spiel-Einheiten (dieselben wie im Wiki), Cooldowns/Lebensdauern in Sekunden. Ausnahme:
`start`/`end` in `RoundModel.groups` zählen in Frames bei 60 fps (Runde 1: 20 Red, `start` 0, `end` 1050.75 ≈ 17,5 s).

### Koordinaten

Karten, Pfade und (später) Turm-Positionen nutzen das Simulations-Koordinatensystem des Spiels:

- Ursprung in der Mitte des Spielfelds, `x` nach rechts, `y` nach unten (Bildschirm), Einheiten wie Reichweiten.
- Spielfeld: die erste Fläche jeder Karte (`areas[0]`, `AreaModel_BaseArea`) ist das Rechteck x −150…150,
  y −116…116.
- Pfade beginnen und enden außerhalb davon. Monkey Meadow (`Tutorial`): Start bei x −333.6, y −18.9 (links
  außerhalb), Ende bei x −17.9, y 315.1 (unten außerhalb), 682 Punkte.
- Die Python-Sim (`btd_sim/`) und ihre Replays rechnen in einem 20×15-Raster (100 px je Zelle auf einem
  2000×1500-Screenshot). Die Umrechnung Raster ↔ Spielkoordinaten ist noch nicht kalibriert; das kommt mit dem
  Recorder (Stufe 2).

**`summary.json`** (Übersicht, aus den Rohdaten abgeleitet):

- `towers[]`: `name`, `baseId`, `tier`, `tiers` ([oben, mitte, unten]), `cost` (Medium), `costByDifficulty`
  (`Easy`/`Medium`/`Hard`/`Impoppable`), `range`, `appliedUpgrades`, `isSubTower`, `isParagon`, …
- `upgrades[]`: `name`, `cost`, `costByDifficulty`, `tier`, `path`, `xpCost`
- `bloons[]`: `name`, `baseId`, `maxHealth`, `speed`, `leakDamage`, Eigenschaften (`isCamo`, `isGrow`,
  `isFortified`, `isMoab`, `bloonProperties`), `children`
- `roundSets`: Anzahl Runden und Gruppen je Runde
- `maps[]`: `id`, `difficulty`

`costByDifficulty` ist nicht aus dem Spiel gelesen, sondern mit `CostHelper` aus Mod Helper berechnet: Medium-Preis
× {0.85, 1, 1.08, 1.2}, auf 5 gerundet. Für Modi mit eigenem Kostenfaktor gelten die Mods in `gamedata/mods.json`
bzw. `matches/`. Upgrade-`tier` und `path` zählen ab 0. Der Freeplay-/Late-Game-Lebenspunkte-Zuwachs für MOABs steht nicht als Tabelle in den Models,
sondern ist Spiel-Code; die Freeplay-Runden selbst stehen in `gamemodel/freeplayGroups.json`.

Der Dump-Ordner bekommt eine eigene `.gitignore` mit `*`, damit die Daten auch in einem Checkout ohne passende
Repo-`.gitignore` nicht versehentlich committet werden.

## Plausibilitätsprüfung gegen das Wiki

```bash
.venv/bin/python mod/tools/compare_dump_wiki.py --dump data/raw/game_dump/57.4
```

Vergleicht Basis und reine Pfad-Tiers jedes Turms mit `data/base/towers/` (Kosten, Reichweite, Camo der
ersten schadensverursachenden Attacke, damage/pierce/cooldown jeder Wiki-Attacke) und schreibt `compare_wiki.md` in
den (gitignored) Dump-Ordner. Stand 57.4: Kosten 416/416 und Reichweite 416/416 gleich, Camo 351/355
(Beast Handler und Village ohne Ballista nicht prüfbar), Attacken 445/500 exakt. Nicht gezählt, aber im Bericht
aufgeführt: Wiki-Attacken ohne `damage` oder mit `damage` 0 (Buffs, Kleber, Effekte) und Attacken von
Sub-Towern (Beast-Handler-Tiere liegen als eigene Türme unter `towers/`). Tests mit synthetischen Daten:
`tests/test_compare_dump_wiki.py`.

Ein Fehler bei einem einzelnen Objekt (auch einer einzelnen Runde oder einer Dateinamen-Kollision) bricht den Dump
nicht ab; er steht in `manifest.json` → `errors` und im Log.
