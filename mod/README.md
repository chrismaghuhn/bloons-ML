# BloonsML-Mod (MelonLoader + BTD6 Mod Helper)

Mod für Bloons TD 6, die die Spieldaten direkt aus den Models des Spiels liest.

- **Stufe 1 (jetzt): Daten-Dump.** Türme mit *allen* gültigen Upgrade-Kombinationen (inkl. Crosspaths wie 2-0-3,
  Helden-Level, Paragons, Sub-Towers), Upgrades, Bloons, Runden-Sets mit Spawn-Timing, Karten-Geometrie und
  Spielversion, als JSON nach `data/raw/game_dump/<version>/`. Das ist eine weitere Rohdatenquelle neben
  `data/raw/towers/` (Wiki), siehe Schichtenmodell in `docs/engine-design.md` §4.
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

Referenziert werden `MelonLoader/net6/*` (MelonLoader, Harmony, Il2CppInterop, Newtonsoft.Json),
`MelonLoader/Il2CppAssemblies/*.dll` und `Mods/Btd6ModHelper.dll`, alles mit `Private=false`.

## Benutzung

| Aktion | Wie |
|---|---|
| Kompletter Dump | **F6** (am besten im Hauptmenü) oder Mods → BloonsML → „Dump“ |
| Karte dumpen | automatisch beim Start jeder Partie; **F7** schreibt die zuletzt geladene Karte erneut |
| Einstellungen | Mods → BloonsML: Dump-Ordner, Hotkeys, Auto-Karten-Dump |

Fortschritt und Fehler stehen im MelonLoader-Log (`BloonsTD6/MelonLoader/Latest.log`, Präfix `[BloonsML]`).
Der Dump dauert etwa eine bis einige Minuten, das Spiel bleibt dabei bedienbar, ruckelt aber.

Für **alle Karten** muss man jede Karte einmal starten (Geometrie wird erst beim Laden einer Partie erzeugt).
Für das Projekt reicht vorerst Monkey Meadow.

## Ausgabeformat

`<DumpRoot>/<Spielversion>/` (Spielversion = `Application.version`, z.B. `57.4`):

| Pfad | Inhalt |
|---|---|
| `manifest.json` | Spiel-/Unity-/Mod-Version, Zeitpunkt, Dauer, Anzahl Dateien je Kategorie, Fehlerliste |
| `summary.json` | Kleine Übersicht (siehe unten) |
| `towers/<baseId>/<name>.json` | Ein `TowerModel` je gültiger Kombination, z.B. `towers/DartMonkey/DartMonkey-203.json`; Helden als `Quincy 1…20`, Paragons, Sub-Towers |
| `upgrades/<name>.json` | `UpgradeModel` (Kosten, Pfad, Tier, XP) |
| `bloons/<baseId>/<name>.json` | `BloonModel` je Variante (z.B. `Red`, `RedCamo`, `CeramicRegrowFortified`, `Moab`, …) |
| `rounds/<RoundSet>/_set.json` | `RoundSetModel` ohne Runden, plus `roundCount` |
| `rounds/<RoundSet>/NNN.json` | `RoundModel` der Runde NNN (1-basiert): `groups[]` mit `bloon`, `start`, `end`, `count`, ggf. `emissions` |
| `maps/_index.json` | Metadaten aller Karten (`id`, `difficulty`, …) aus `GameData.mapSet` |
| `maps/<mapName>.json` | `{gameVersion, mapName, difficultyAtDump, map: MapModel}`: Pfade (`paths[].points[].point`), Flächen (`areas[]` mit `polygon` und `type`), Blocker, Spawner |
| `gamemodel/<feld>.json` | Alle übrigen Felder von `GameModel` einzeln (z.B. Powers, Wissensbaum, Buff-Indikatoren, globale Konstanten) |

**Rohformat:** Jede Datei ist eine möglichst unveränderte Serialisierung des Spiel-Models über den Serializer von
Mod Helper (`Il2CppJsonConvert`). Jedes Objekt trägt `"$type"` mit Typname und Assembly, z.B.
`Il2CppAssets.Scripts.Models.Towers.Projectiles.Behaviors.DamageModel, Assembly-CSharp` (zum Vergleichen am Komma
trennen). Ein Turm enthält also seine komplette
Behavior-Hierarchie: `AttackModel` → `weapons[]` (`WeaponModel`, `Rate` = Cooldown in s) → `projectile`
(`ProjectileModel`, `pierce`, `radius`) → `behaviors[]` (`DamageModel` mit `damage` und `immuneBloonProperties`,
`TravelStraitModel` mit `Speed`/`Lifespan`, `CreateProjectileOnContactModel` für Explosionen,
`DamageModifierForTagModel` für Bonusschaden gegen MOAB/Keramik/…, Filter für Camo usw.). Reichweite: `range`
am Turm und an den Attacks; Camo-Erkennung über `FilterInvisibleModel`/Detection-Behaviors.
Feldnamen entsprechen den Feldern im Spiel und können sich mit Updates ändern.

Einheiten: Distanzen in Spiel-Einheiten (dieselben wie im Wiki), Cooldowns/Lebensdauern in Sekunden.  Ausnahme:
`start`/`end` in `RoundModel.groups` zählen in Frames bei 60 fps (Runde 1: 20 Red, `start` 0, `end` 1050.75 ≈ 17,5 s).

**`summary.json`** (Übersicht, aus den Rohdaten abgeleitet):

- `towers[]`: `name`, `baseId`, `tier`, `tiers` ([oben, mitte, unten]), `cost` (Medium), `costByDifficulty`
  (`Easy`/`Medium`/`Hard`/`Impoppable`), `range`, `appliedUpgrades`, `isSubTower`, `isParagon`, …
- `upgrades[]`: `name`, `cost`, `costByDifficulty`, `tier`, `path`, `xpCost`
- `bloons[]`: `name`, `baseId`, `maxHealth`, `speed`, `leakDamage`, Eigenschaften (`isCamo`, `isGrow`,
  `isFortified`, `isMoab`, `bloonProperties`), `children`
- `roundSets`: Anzahl Runden und Gruppen je Runde
- `maps[]`: `id`, `difficulty`

Kosten je Schwierigkeit sind der Medium-Preis × {0.85, 1, 1.08, 1.2}, auf 5 gerundet (`CostHelper` von Mod Helper,
wie im Spiel). Der Freeplay-/Late-Game-Lebenspunkte-Zuwachs für MOABs steht nicht als Tabelle in den Models,
sondern ist Spiel-Code; falls er als Konstante im GameModel steckt, liegt er unter `gamemodel/`.

Der Dump-Ordner bekommt eine eigene `.gitignore` mit `*`, damit die Daten auch in einem Checkout ohne passende
Repo-`.gitignore` nicht versehentlich committet werden.

## Plausibilitätsprüfung gegen das Wiki

```bash
python mod/tools/compare_dump_wiki.py --dump data/raw/game_dump/57.4
```

Vergleicht Basis und reine Pfad-Tiers jedes Turms mit `data/base/towers/` (Kosten, Reichweite, Camo der
Hauptattacke, damage/pierce/cooldown jeder Wiki-Attacke) und schreibt `compare_wiki.md` in den (gitignored)
Dump-Ordner. Stand 57.4: Kosten 416/416 und Reichweite 416/416 gleich, Camo 413/416, Attacken 463/608.

Ein Fehler bei einem einzelnen Objekt (auch eine Dateinamen-Kollision) bricht den Dump nicht ab; er steht in `manifest.json` → `errors` und im Log.
