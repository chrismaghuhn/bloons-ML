# Design: mehr Türme, Schritt 3 – Alchemist, Engineer, Mortar, Dartling, Spike Factory, Heli, Ace (freigegeben 2026-10-27 mit Anpassungen, §11)

Ziel: von 14 auf 21 Türme. Das sind alle Landtürme, die für CHIMPS auf Monkey Meadow noch fehlen; Wassertürme
(Sub, Buccaneer, Mermonkey) bleiben draußen, Beast Handler und Helden auch. Die sieben Türme brauchen
deutlich mehr neue Mechanik als Schritt 1 und 2. Dieses Dokument ordnet sie deshalb in **sieben gemeinsame
Bausteine** (§2, B1–B7), die jeweils mehrere Türme freischalten, und schlägt eine Reihenfolge nach CHIMPS-Nutzen vor (§5).

Wichtigste Befunde vorab:
- **Engineer 0-0-0 geht schon heute** durch den Konverter (6 Kombinationen ohne neue Engine-Arbeit).
- **Ein gemeinsamer "Wurf"-Baustein** (Projektil fliegt zu einem Punkt, ohne unterwegs zu treffen, und wirkt
  dort) trägt Alchemist, Mortar, Spike Factory, Engineer-Sentries/-Schaum und Ace-Ananas.
- **Zielpunkte ohne neue Aktionsdimension:** Mortar zielt auf einen festen besten Punkt je Karte (meiste
  Streckenlänge im Explosionsradius, §3); Dartling zielt automatisch auf den vordersten Bloon
  (§3). Der Aktionsraum wächst nur um die neuen Turmtypen.
- **Alles Zeitliche bleibt innerhalb der Runde** (Tränke, Spikes, Sentries, Flieger-Position) und wird zu
  Rundenbeginn zurückgesetzt, wie heute die Cooldowns. Damit ändern sich `state_hash`, Startzustände und
  Observation nicht (§6). Einzige bewusste Lücke: Long-Life-Spikes überdauern das Rundenende nicht.
- **Abilities bleiben gesperrt**, auch Overclock, das in CHIMPS viel genutzt wird (Frage 4).

Zahlen aus dem Dump stehen hier nur als Regeln oder Belege. Die konkreten Werte holt der Konverter.
Rohdaten bleiben in `data/raw/game_dump/` (gitignored).

## 1. Was die Türme im Dump nutzen (57.4)

Ausgewertet mit `check_types` über alle 64 Varianten je Turm (Basis + 63 Upgrade-Kombinationen; Paragon-Dateien von Engineer, Spike und Ace bleiben außen vor). "Abdeckung"
heißt: Die Variante enthält nach dem Baustein keine unbekannten Modell-Typen mehr. Das ist eine **Obergrenze**:
Feld-Prüfungen der STRICT-Modelle können einzelne Varianten noch ablehnen, wie in Schritt 2 (dort 44 statt 49
bei der Farm). Heute sind nur Engineer 000, 001, 010, 011, 020 und 021 konvertierbar. Wo ein Modell schon heute STRICT bekannt ist (z.B. `RemoveBloonModifiersModel`, nur mit allen Schaltern aus), zählt es in der Hochrechnung als abgedeckt; solche Varianten kommen erst mit dem passenden Baustein (hier B6).

### 1.1 Alchemist (magic, 550 $, Reichweite 45)

Basis: Trank fliegt zur Zielposition (`ArriveAtTargetModel`, `altSpeed` 250, ohne Treffer im Flug) und platzt dort
(`CreateProjectileOnExhaustFraction` → Spritzer: Radius 14, Pierce 15, Schaden 1, dazu Säure-DoT als
`AddBehaviorToBloonModel` + `DamageOverTimeModel`). `AcidPoolModel` am Spritzer (Lebensdauer 0,05 s; beim Fehlwurf
Radius 7 und `lifespanIfMisses` 0) → ignorieren [KAL]. Der Spritzer wird wie heute in `convert_contact` als
Sofort-Explosion behandelt; sein `AgeModel` (7 s) ist keine Liegezeit. `CanBuffIndicatorModel` ist Anzeige.

| Pfad/Upgrade | Modelle (neu) | Wirkung | Vorschlag |
|---|---|---|---|
| 2xx Acidic Mixture Dip | `TargetFriendlyModel`, `AddAcidicMixtureToProjectileModel` (`cap` 30), `AcidicMixtureCheckModel` (`maxCount` 10), `DestroyIfTargetLostModel` | zweiter Angriff alle 10 s auf einen befreundeten Turm mit Waffe (nicht Farm/Village/Alchemist/…): dessen Projektile bekommen +1 Schaden gegen Ceramic, MOABs, Fortified-Lead | **Zeit-Effekt** (§2.4), Ladungen = Schüsse [KAL: `cap` 30 oder `maxCount` 10] |
| 3xx Berserker Brew, 4xx Stronger Stimulant | `BrewTargettingModel`, `AddBerserkerBrewToProjectileModel`, `BerserkerBrewCheckModel` | alle 8 s ein Turm in Reichweite: +1 Schaden, +2 Pierce, +10 % Tempo, +10 % Reichweite für 5 s oder 25 Angriffe (4xx: +3 Pierce, +15 %/+15 %, 12 s/40 Angriffe); danach 5 s Sperre (`rebuffBlockTime`) | Zeit-Effekt (§2.4) |
| 5xx Permanent Brew | `LoadAlchemistBrewInfoModel`, `RemovePermaBrewModel` | Brew ohne Ablauf | gesperrt (Phase 2, braucht Zustand über Runden) |
| x2x Perishing Potions | `RemoveBloonModifiersModel` (nur `cleanseFortified`, ohne MOABs) | entfernt Fortified | Bloon-Modifier entfernen (§2.6) |
| x3x Unstable Concoction | `UnstableConcoctionSplashModel`, `DamageModifierUnstableConcoctionModel` | MOAB-Angriff, Explosion beim Platzen | später (A4) |
| x4x/x5x | `AbilityModel`, `MorphTowerModel` | Ability | gesperrt |
| xx2 Acid Pool | `AcidPoolEmissionModel`, `TargetTrackOrDefaultAcidPoolModel` | Säurepfütze auf der Strecke | **Bodenobjekt** (§2.3) |
| xx3 Lead to Gold | `GiveCashOnPopModel` (50 $ je geplatztem Lead, `flatCashAmount`) | Cash je Pop | gesperrt, bis geklärt ist, ob CHIMPS es zulässt (§2.6) |
| xx4 Rubber to Gold | `RemoveBloonTypeModifierModel`, `FilterMutatedTargetModel` | Cash auf alle Bloons | gesperrt, bis die CHIMPS-Zulässigkeit geklärt ist (§2.6) |
| xx5 Bloon Master Alchemist | `MorphBloonModel` | MOABs zu Rot | gesperrt |

Abdeckung: Wurf allein **10**, + Tränke **22**, + Bodenobjekte/Gold **39** (ohne Lead/Rubber to Gold 29), + Unstable **44** von 64.

**Stapelregel (Vorgabe aus Schritt 2 geprüft):** Tränke stapeln nicht. Belege:
- Brew: `mutationId` je Stufe ("BeserkerBrew", "BerserkerBrewBetter", "BerserkerBrewPerma") und
  `ignoreMutationsByOrder` in genau dieser Reihenfolge. Ein Turm mit gleich starkem oder stärkerem Brew wird nicht
  anvisiert; der stärkere ersetzt den schwächeren (`mutatorsToRemove`). Zwei Alchemisten geben also höchstens den
  stärksten Brew, nicht beide.
- Dip: `TargetFriendlyModel.mutationId` "AcidicMixtureBuff" → ein getränkter Turm wird erst nach Ablauf neu getränkt.
- Brew und Village sind verschiedene Mutatoren und wirken zusammen (z.B. Village-Tempo × Brew-Tempo)
  [KAL: Verrechnung, multiplikativ vorgeschlagen].

### 1.2 Engineer Monkey (support, 350 $, Reichweite 40)

Basis wie ein Dart-Schütze (Nagel, Pierce 3) – heute schon konvertierbar.

| Pfad/Upgrade | Modelle (neu) | Wirkung | Vorschlag |
|---|---|---|---|
| 1xx Sentry Gun (+2xx/3xx Tempo) | `CreateTowerModel` (Sub-Turm "Sentry"), `RandomPositionModel` (8–40 vom Turm, Land, nahe Strecke), `TowerExpireModel` (25 s), `CreditPopsToParentTowerModel`, `SavedSubTowerModel` | alle 10 s wird ein Sentry (eigener Angriff, Reichweite 45) geworfen | **Sub-Türme** (§2.5) |
| 4xx Sentry Expert | `CreateTypedTowerModel`, `…CurrentIndexModel` | reihum vier Sentry-Arten (Boom, Cold, Energy, Crushing) | Sub-Türme mit Liste |
| 5xx Sentry Champion | `CreateProjectileOnTowerDestroyModel`, `…OnExpireModel` | Explosion beim Ablauf | später |
| x2x Deconstruction | – | Bonus gegen MOAB/Fortified | geht schon |
| x3x Cleansing Foam | `TargetSelectedPointModel` (nicht wählbar, `startWithClosestTrackPoint`), `RandomTargetSpreadModel`, `RemoveBloonModifiersModel` (Camo, Regrow, Lead), `StartOfRoundRateBuffModel` | Schaum auf den nächsten Streckenpunkt, entfernt Camo/Regrow/Lead | Wurf + Bodenobjekt + Modifier entfernen |
| x4x/x5x Overclock/Ultraboost | `AbilityModel`, `OverclockModel` | Ability | gesperrt (Frage 4) |
| xx2 Pin | `SlowOnPopModel` ("Stun:Weak", 1 Schicht) | kurzer Stopp der Kinder | Status-Effekt am Pop [KAL: Dauer] |
| xx3 Double Gun | `EmissionWithOffsetsModel` (2 Marker) | zwei Nägel nebeneinander | als `Parallel` (Abstand aus den Markern) |
| xx4/xx5 Bloon Trap | `EatBloonModel`, `LimitProjectileModel`, `CashModel`, `PickupModel` | Falle frisst Bloons bis Kapazität, Cash beim Einsammeln | später (E4) |

Abdeckung: heute **6**, + Pin/Double Gun **12**, + Sentries **34**, + Schaum/Rundenstart **39**, + Falle **44**.

Stapelregel: `StartOfRoundRateBuffModel` hat `isUnique` und eine feste `mutatorId` und wirkt nur auf den eigenen
Turm. Overclock ist gesperrt; sobald Abilities kommen, gilt dieselbe Regel (ein Overclock je Turm).

### 1.3 Mortar Monkey (military, 600 $)

Basis: `AttackModel.range` 9 999 999 (ganze Karte), Ziel ist ein **gewählter Punkt** (`TargetSelectedPointModel`,
wählbar). Streuung `RandomTargetSpreadModel` (Radius 18, mit xx1 kleiner). Granate: `InstantModel` + `AgeModel`
0,5 s → Explosion (Radius 20, Pierce 25, Schaden 2, nicht Black) nach 0,5 s am Punkt. `RotateToPointerModel` und
`ChipMapBasedObjectModel` ("ChipCaveMonkey", kartenspezifisch) sind ohne Wirkung.

| Pfad | Modelle (neu) | Vorschlag |
|---|---|---|
| 1xx/2xx | – | gehen mit Zielpunkt |
| 3xx Shockwave | `DamageInRingRadiusModel` | Zusatzschaden im inneren Ring (M2) |
| x3x Heavy Shells | `DamageModifierForBloonStateModel` | Bonus gegen Status (M2) |
| x4x/x5x | `AbilityModel`, `TurboModel`, `DamageModifierSupportModel` | gesperrt |
| xx2 Burny Stuff | DoT (schon bekannt) | geht |
| xx3 Signal Flare | `RemoveBloonModifiersModel` (`cleanseCamo`) | Modifier entfernen (§2.6) |
| xx4 Shattering Shells | `FilterWithTagModel` | geht |
| xx5 Blooncineration | `RefreshPierceModel`, `ProjectileZeroRotationModel` | M3 |

Abdeckung: Zielpunkt **29**, + Ring/Status **49**, + Blooncineration **54**.

### 1.4 Dartling Gunner (military, 850 $)

Basis: `isGlobalRange`, schießt in Richtung des **Mauszeigers** (`TargetPointerModel` "Normal") oder auf einen
festen Punkt (`TargetSelectedPointModel` "Locked"), Drehrate 180 °/s (`RotateToPointerModel`). Streuung
`RandomEmissionModel` 23° (Mittelwert wie in Schritt 1). Pfeil fliegt 350 × 1,14 ≈ 400 Einheiten, also über die
ganze Karte. `DartlingMaintainLastPosModel` hält die letzte Richtung.

| Pfad | Modelle (neu) | Vorschlag |
|---|---|---|
| 1xx | – | geht mit Ziel-Regel |
| 2xx Laser Shock | `DamageModifierForBloonStateModel` + DoT | D2 |
| 4xx Plasma/5xx Ray of Doom | `LineProjectileEmissionModel` (Strahl) | D4: Sofort-Treffer entlang einer Linie |
| x3x Hydra Rockets | `CreateProjectileOnExhaustPierceModel`, `EmissionWithOffsetsModel` | D3 |
| x4x/x5x | Ability, `TravelCurvyModel` | gesperrt |
| xx3 Buckshot | `TravelStraitSlowdownModel`, `KnockbackModel` | D3 |
| xx4 BADS | `TargetIndependantModel` | D3 |

Abdeckung: Ziel-Regel **14**, + Laser **24**, + Buckshot/Hydra **44**, + Strahl **54**.

### 1.5 Spike Factory (support, 1000 $, Reichweite 34)

Basis: wirft alle 1,75 s einen Spike-Stapel auf einen **zufälligen Streckenpunkt in Reichweite**
(`TargetTrackModel`, Versatz bis 3). Wurf mit fester Flugzeit 0,9 s, ohne Treffer im Flug. Der Stapel liegt dann:
Pierce 5, Radius 6, `ClearHitBloonsModel` 0,001 (kann denselben Bloon wieder treffen), Kollisionsprüfung nur alle
0,083 s (`checkCollisionInterval` = 5 Ticks), Lebensdauer 50 s **Rundenzeit** (`AgeModel.useRoundTime`), am
Rundenende weg. Ohne Wirkung: `FadeProjectileModel` (Darstellung), `RandomRotationWeaponBehaviorModel`,
`EndOfRoundClearBypassModel` (nur Modus Apopalypse), `ExpireProjectileOnBossSpawnedModel` (keine Bosse).

| Pfad | Modelle (neu) | Vorschlag |
|---|---|---|
| 1xx/2xx | – | gehen |
| 3xx Spiked Balls | `DamageModifierForBloonTypeModel` | S2 |
| 4xx Spiked Mines | `CreateProjectileOnExpireModel` + DoT | S2: Explosion beim Verbrauch |
| x1x/x2x Faster Production | – | gehen |
| x3x MOAB SHREDR | – | geht |
| x4x/x5x Spike Storm/Carpet | Ability | gesperrt |
| xx2 Directed Spikes | `Smart/Auto/CloseTargetTrackModel`, `TargetSelectedPointModel`, `StartOfRoundRateBuffModel` (+25 % für 3 s) | S3 |
| xx3 Long Life (`AgeModel.rounds` 1, 140 s) | – | Lebensdauer ja, **Überdauern des Rundenendes nein** (§6) |
| xx4/xx5 | – | gehen |

Abdeckung: Spikes (S1) **18**, + Bonus/Minen (S2) **26**, + Directed (S3) **49**, + Super Mines (S4,
`CreateProjectileOnExhaustPierceModel`) **54**.

### 1.6 Heli Pilot (military, 1500 $)

Turm = Landeplatz (Rechteck 27 × 27), der Heli (`AirUnitModel`, `HeliMovementModel` max. Tempo 40) ist eine
eigene, bewegliche Einheit. Modi (wählbar): Lock in Place, Follow Touch, Patrol Points, ab 2xx Pursuit (verfolgt
Bloons). Angriff vom Heli aus, Reichweite 42 (statt 22 am Turm), 2 Pfeile über zwei Abwurfmarker.

| Pfad | Modelle (neu) | Vorschlag |
|---|---|---|
| Basis, 1xx, x1x/x2x, xx1/xx2 | AirUnit/Heli/Follow/Lock/Patrol, `ResetRateOnInitialiseModel` | H1: Heli steht über dem Landeplatz (Lock) |
| 2xx Pursuit | `PursuitSettingModel` | H2: fliegt mit Höchsttempo zum vordersten Bloon |
| 3xx Razor Rotors, 4xx/5xx Apache | `TrackTargetModel` (schon da), `PrioritiseRotationModel`, `SetTriggerOnAirUnitFireModel` | H3 |
| x3x Downdraft | `WindModel` | gesperrt (Wind wie in Schritt 1) |
| x4x/x5x Chinook/Special Poperations | Ability | gesperrt |
| xx3 MOAB Shove | `MoabShoveZoneModel` | H2 |
| xx4/xx5 Comanche | `ComancheDefenceModel` (Mini-Helis) | H3 |

Abdeckung: Lock **14**, + Pursuit/Shove **29**, + Comanche/Apache **49**.

### 1.7 Monkey Ace (military, 800 $)

Turm = Startbahn (Rechteck 28 × 18), das Flugzeug fliegt eine feste Bahn (`PathMovementModel`, Tempo 60):
Kreis (Radius 80), Acht um den Turm (`FigureEightPatternModel`, Radius 40, `useTowerPosition`; dazu dieselbe Acht
gedreht als ∞), ab xx2 zusätzlich wählbar eine Ellipse um die Kartenmitte (`CenterElipsePatternModel`). Basis-Angriff ohne Ziel (`fireWithoutTarget`): 8 Pfeile rundum alle 1,68 s.

| Pfad | Modelle (neu) | Vorschlag |
|---|---|---|
| Basis, 1xx/2xx, xx1/xx2 | AirUnit, Patterns, `AttackAirUnitModel` | C1: Acht um den Turm |
| 3xx Fighter Plane | `TargetStrongAirUnitModel`, `AccelerateModel`, Zielsuche | C3 |
| x1x Pineapples, x3x Bomber | `FallToGroundModel`, `EmissionOverTimeModel`, `CheckAirUnitOverTrackModel` | C2 (Wurf mit Verzögerung) |
| x4x/x5x Ground Zero/Tsar | Ability | gesperrt |
| xx3 Neva-Miss | `AdoraTrackTargetModel` | C3 (Zielsuche) |
| xx4/xx5 Spectre/Fortress | `AlternateProjectileModel` | C4 |

Abdeckung: Flugbahn **9**, + Ananas/Bomber **24**, + Neva-Miss/Fighter **42**, + Spectre **54**.

## 2. Neue Engine-Bausteine

Alle Bausteine sind optionale Felder in `btd-data` (`deny_unknown_fields`, Standard = heutiges Verhalten). Fehlen sie
in den Daten, läuft der Tick-Loop genau wie bisher (Parität, §6).

### 2.1 Wurf (B1)

Neue Lieferart `Delivery::Lob {time | speed, spread, on_arrive}` (zuerst als Option an `Delivery::Projectile`
geplant; als eigene Lieferart in Teil 1a umgesetzt, weil Tempo, Lebensdauer und Radius eines Flugs dort keinen
Sinn ergeben). Das Projektil fliegt geradlinig zu einem
**Zielpunkt** (Position des Ziels beim Abwurf bzw. der Zielpunkt aus §2.2), trifft unterwegs nichts
(`filterCollisionWhileMoving`) und löst bei Ankunft `on_exhaust` aus bzw. wird zum Bodenobjekt (§2.3).
- `time`: feste Flugzeit (Spike 0,9 s, Mortar 0,5 s), `speed`: Flugzeit = Abstand / Tempo (Alchemist 250, Sentry 80).
- `ttl` in Ticks wie bisher ganzzahlig (`ticks_of`), damit f32/f64 dasselbe tun.
- Die Kurve (`ArriveAtTarget.curveSamples`, Höhe) ist Darstellung, nur die Flugzeit zählt [KAL: ob der Spike nach
  0,9 s oder entlang der Kurve früher "liegt"].
- Mortar: `InstantModel` + `AgeModel` = Wurf mit `time` = Lebensdauer.
- Streuung (`RandomTargetSpreadModel`): ohne Zufall Mittelwert (genau der Punkt), mit `randomness` gleichverteilt im
  Kreis, aus `Game::rng` (wie die Crits).

### 2.2 Ziel-Arten (B2)

Heute zielt jeder Angriff auf "First in Reichweite". Neu: das vorhandene Enum `AttackV1.targeting` (heute nur `first`)
bekommt weitere Varianten, statt ein zweites Feld anzulegen; Standard bleibt `first`:

| `targeting` | Ziel | Türme |
|---|---|---|
| `first` | wie heute | alle bisherigen, Alchemist-Wurf |
| `first_global` | vorderster sichtbarer Bloon auf der ganzen Karte; Richtung dorthin, Drehrate begrenzt | Dartling (§3) |
| `track_nearest` | fester Streckenpunkt, der dem Turm am nächsten liegt; bei Gleichstand der mit der größeren Pfad-Distanz | Engineer-Schaum (`startWithClosestTrackPoint`) |
| `track_best` | fester Punkt je Karte und Explosionsradius: der Streckenpunkt mit der meisten Streckenlänge im Radius; bei Gleichstand der mit der größeren Pfad-Distanz | Mortar (§3) |
| `track_spread` | Streckenpunkt in Reichweite, über die Streckenlänge in Reichweite verteilt | Spike Factory, Acid Pool |
| `friendly` | befreundeter Turm in Reichweite nach fester Regel (§2.4) | Alchemist-Tränke |
| `none` | feuert, solange die Runde läuft, ohne Ziel (Richtung fest) | Ace |

- Wann ein punkt- oder streckenzielender Angriff feuert: solange mindestens ein für ihn anvisierbarer Bloon
  (Camo-Regel) auf der Karte ist. Das gilt auch für die Spike Factory (Dump: `fireWithoutTarget` aus,
  Camo-Filter am Angriff) [KAL]. Nur die Ace feuert ohne Ziel (`fireWithoutTarget` an), solange die Runde läuft;
  dafür darf die Turm-Schleife nicht mehr bei 0 Bloons übersprungen werden (heute `if !self.bloons.is_empty()`),
  wohl aber für alle Angriffe ohne `none`, damit die Parität hält.
- `track_spread` ohne Zufall: Die Strecke in Reichweite wird je Turm als Liste von Intervallen (Distanz
  entlang des Pfads) gespeichert und neu berechnet, wann immer sich die effektive Reichweite ändert
  (`apply_buffs`: Village-Aura, Brew mitten in der Runde). Der k-te Wurf der Runde nimmt den Punkt bei Anteil `frac(k × φ)` (Goldener
  Schnitt, gleichmäßig und deterministisch) der Gesamtlänge. Der Zähler beginnt jede Runde bei 0. Mit
  `randomness` gleichverteilt. Der Versatz quer zur Strecke (`maxOffset` 3) entfällt ohne Zufall.
- Directed Spikes (xx2) bietet Smart/Auto/Close. Vorschlag: auch dort `track_spread`, nur der Rundenstart-Bonus
  kommt dazu. Close-Modus (Stapel nah am Turm) wäre eine spätere Verfeinerung [KAL: welcher Modus stärker ist].

### 2.3 Bodenobjekte (B3)

Gelandete Würfe, die liegen bleiben: Spike-Stapel, Säurepfützen (xx2), Engineer-Schaum, später Falle und Minen.
- Eigene Liste `Game::ground` (nicht `projectiles`), Felder: Position, Radius, Pierce, Schaden, Extras,
  Lebensdauer in Ticks, Kollisions-Intervall (`checkCollisionInterval` in Ticks, Standard 1), Treffer-Reset
  (`clear_hits`, vorhanden), `on_exhaust` (Spiked Mines).
- Treffer wie fliegende Projektile (Kollisionsabstand Radius + Bloon-Radius, Pierce-Abzug, Filter), aber ohne Bewegung.
- **Rundenende:** Bodenobjekte halten eine Runde **nicht** am Laufen (heute endet sie erst, wenn
  `projectiles` leer ist). Am Rundenende werden sie gelöscht (Spiel: Spikes verschwinden; Long Life siehe §6).
- Leistung: Spike Factories erzeugen viele Objekte (spät > 100). Jedes Objekt merkt sich die Pfad-Distanz seines
  Landepunkts; die Kollision prüft zuerst ein Fenster in der Bloon-Distanz und erst dann den 2D-Abstand. Kreuzt die
  Strecke sich selbst, gibt es je Objekt mehrere Fenster (vorab berechnet). Reihenfolge der Prüfung fest
  (Erzeugungsreihenfolge), damit das Ergebnis deterministisch ist.

### 2.4 Zeit-Effekte auf Türmen (B4)

Die zweite Buff-Art aus `engine-design.md` (`timed_effects`). Bisher ändern sich effektive Werte nur bei Aktionen
(`refresh_buffs`); Tränke und der Rundenstart-Bonus ändern sie **mitten in der Runde**.
- `Tower.timed: SmallVec<TimedEffect>` mit `group` (Mutator), Werten (Schaden +, Pierce +, Tempo, Reichweite,
  Tag-Bonus wie `damage_vs`), Rest-Ticks, Rest-Schüsse und Sperr-Ticks danach.
- **Revision von Schritt 2 §2.1** ("im Tick-Loop läuft es nie"): `refresh_buffs` bleibt bei Aktionen, aber
  effektive Werte einzelner Türme werden jetzt auch im Tick-Loop neu geschrieben.
- Ereignisgesteuert, nicht pro Tick: Beim Start und beim Ende eines Effekts werden nur für diesen Turm die
  effektiven Werte neu geschrieben, mit derselben Funktion wie die Auren (`Tower::apply_buffs`, erweitert um
  Schaden und Tag-Bonus). Laufzeit-Zustand bleibt erhalten (wie in Schritt 2). Die Rest-Ticks zählen im
  Tick-Loop herunter (eine Ganzzahl je Effekt).
- Reichweite durch Brew ändert **keine Aura-Radien** (Village ist ausgeschlossen, `towerIgnoreList`) und keine
  Rabatt-Zonen. Damit bleibt der Fixpunkt aus Schritt 2 unberührt.
- Stapeln (§1.1): je `group` höchstens ein Effekt; ein stärkerer (Reihenfolge `ignoreMutationsByOrder`) ersetzt
  den schwächeren, ein gleicher oder schwächerer wird nicht vergeben. Zeit-Effekt × Aura: verschiedene Gruppen,
  Tempo multiplikativ, Zuschläge additiv [KAL].
- "Angriffe" für Rest-Schüsse: jeder Schuss irgendeines Angriffs des Turms zählt eins [KAL].
- Tempo `rateUp` 0,15 heißt Cooldown ÷ 1,15 [KAL: oder × 0,85].
- Pierce-Zuschlag wie bei der Village nur für fliegende Projektile (nicht Sofort-Treffer, nicht Flächen).
- **Zielwahl `friendly`** (Brew, Dip): Kandidaten sind Türme in Reichweite mit Waffe, nicht auf der
  Ausschlussliste des Dumps, ohne gleich starken Effekt und nicht gesperrt. Gewählt wird der mit dem höchsten
  `spent` (Stellvertreter für "stärkster Turm"), bei Gleichstand die kleinste Turm-ID. Ohne Kandidat feuert der
  Angriff nicht und bleibt bereit [KAL: Regel im Spiel].
- `StartOfRoundRateBuff` (Engineer x3x, Spike xx2): Zeit-Effekt, der zu Rundenbeginn auf dem eigenen Turm startet.
- Zu Rundenbeginn werden alle Zeit-Effekte gelöscht (§6).

### 2.5 Sub-Türme (B5)

Engineer-Sentries (später Comanche-Mini-Helis, Sentry Champion).
- Eigene Liste `Game::subs` (nicht `towers`: keine Aktion, kein Verkauf, nicht in der Observation, nicht im Hash).
  Ein Sub-Turm hat Position, Variante (aus dem eingebetteten `TowerModel`, vom Konverter in einem eigenen Feld
  `sub_towers: Vec<VariantV1>` am erzeugenden Angriff gespeichert, nicht in `variants`, deren Index die
  Tier-Kombination ist, und nicht als eigener Turmtyp; Sentry Expert hat dort vier Einträge), Laufzeit-Angriffe und Rest-Ticks
  (`TowerExpireModel`).
- Schießen im selben Loop wie Türme, nach allen Türmen, in Erzeugungsreihenfolge.
- Auren: Ein Sub-Turm bekommt beim Erscheinen die statischen Auren an seiner Position (`Game::aura_at`), keine
  Zeit-Effekte (Brew zielt nicht auf Sub-Türme, `isOnSubTower` aus) [KAL].
- **Position** (`RandomPositionModel`: Land, 8–40 vom Engineer, möglichst nah an der Strecke): Der Konverter bzw.
  `Game::new` berechnet je Engineer-Position eine feste Kandidatenliste (Rasterpunkte im Ring auf Land, sortiert
  nach Abstand zur Strecke). Ohne Zufall reihum durch die besten n Kandidaten, mit Zufall gleichverteilt. Sentries
  blockieren keinen Bauplatz (`doesntBlockTowerPlacement`).
- Wurf-Zeit bis zum Erscheinen (`altSpeed` 80) über B1.
- Sentry Expert: Liste von Varianten, reihum (`CreateTypedTowerCurrentIndexModel`); der Index beginnt jede Runde
  bei 0.
- Pops der Sub-Türme zählen wie alle Pops (`CreditPopsToParentTower` ist nur Statistik).

### 2.6 Bloon-Modifier und Cash je Pop (B6)

- `RemoveBloonModifiersModel` heute nur mit allen Schaltern aus erlaubt. Neu als Treffer-Extra `cleanse: {camo,
  regrow, lead, fortified, only_if_damaged, exclude_tags}`: löscht die Flags am getroffenen Bloon (Kinder erben den
  Zustand wie heute die Flags). Lead entfernen heißt: der Bloon verliert die Eigenschaft Lead [KAL: im Spiel wird
  er zu Black?]. Regrow wird noch nicht simuliert, das Flag zu löschen ist harmlos.
- `GiveCashOnPopModel` (Lead to Gold): Bloon-Effekt mit Lebensdauer; platzt der Bloon in dieser Zeit, gibt es
  `flatCashAmount`. Heute wird Pop-Cash einmal am Rundenende gutgeschrieben
  (`pops_total × cash_per_pop`). Vorschlag: Summe der Gold-Beträge in der Runde mitzählen und am Rundenende im
  selben Schritt über `Money::from_f64` gutschreiben (mitten in der Runde wird nichts gekauft; paritätssicher).
  **Zurückgestellt:** Ob CHIMPS diesen Bonus zulässt, zeigt der Dump nicht (`ChimpsModModel` im Mod `Clicks` hat
  keine Felder; kein anderer Mod nennt Cash-Boni). Laut Spielwissen sperrt CHIMPS alle Einkommen außer Pops und
  Rundenbonus, vermutlich also auch Lead/Rubber to Gold. Deshalb baut B6 vorerst nur das Entfernen der
  Modifier; Lead to Gold und Rubber to Gold bleiben gesperrt, bis eine CHIMPS-Partie (Recorder) es klärt. Für
  Medium/Hard kann Cash je Pop danach nachgezogen werden.
- `SlowOnPopModel` (Engineer Pin): Status-Effekt an den Kindern eines geplatzten Bloons, abgebildet mit dem
  vorhandenen Einfrieren/Verlangsamen als kurzer Stopp [KAL: Dauer]; kleiner Teil von B6, kommt aber schon in Teil 1a.

### 2.7 Flieger (B7)

- Ein Turm mit Flieger hat eine zweite Position `unit: (x, y, Richtung)`, von der aus seine Angriffe zielen und
  feuern. Ohne Flieger ist `unit` die Turmposition (Parität).
- **Ace:** Bahn als vorab berechnete Polylinie (Acht um den Turm mit Radius aus dem Dump, ungedreht; auch ab xx2,
  die Ellipse wird nicht gewählt), Position
  = Bogenlänge × Tempo × Zeit. Start zu Rundenbeginn am Bahnanfang. Start/Steigflug (`takeOffTime`) ignoriert [KAL].
  Kreis-Muster nicht genutzt (Vorschlag: Acht, weil sie am Turm hängt und der Agent sie über den Bauplatz wählt).
- **Heli:** H1 steht über dem Landeplatz (Lock in Place am Landeplatz). H2 (Pursuit): fliegt je Tick mit
  Höchsttempo (`maxSpeed`, Upgrades erhöhen es) auf den vordersten Bloon zu, bis zum Abstand `pursuitDistance` (Dump: 32) [KAL: Ausweichen/
  `strafeDistance`, Abstoßung zwischen Helis ignoriert]. Zu Rundenbeginn zurück über den Landeplatz.
- Flieger zählen nicht zur Kollision mit Türmen; der Footprint ist nur Landeplatz/Startbahn (Rechteck wie die Farm,
  Außenkreis-Regel aus Schritt 2 §2.3).

## 3. Aktionsraum: Zielpunkte ohne neue Dimension

Mortar und Dartling haben im Spiel einen frei gewählten Zielpunkt bzw. folgen dem Mauszeiger. Eine echte
Ziel-Aktion würde den Aktionsraum stark vergrößern (z.B. +720 Aktionen "Ziel in Zelle c für Turm X", dazu die
Frage, für welchen Turm). Vorschlag für diesen Schritt:

- **Mortar: `track_best`** (Änderung nach Freigabe; der erste Vorschlag `track_nearest` hätte den Mortar zum
  Nahbereichsturm gemacht, weil der nächste Streckenpunkt immer direkt neben ihm liegt). Ziel ist der Punkt der
  Strecke mit der meisten Streckenlänge innerhalb des Explosionsradius, unabhängig vom Bauplatz. Das ist eine
  Vorab-Tabelle je Karte und Radius (Bigger Blast vergrößert den Radius), berechnet in `Game::new` aus der
  Strecke; Gleichstand nach größerer Pfad-Distanz. Auf Monkey Meadow ist das eine Kurve oder Schleife. So bleibt
  der eigentliche Vorteil des Mortars erhalten (weit weg stehen, dichte Stelle treffen), und die Bridge setzt einen
  festen Punkt je Karte und Radius. Rückfall, falls zu stark: bester Punkt innerhalb eines Radius R um den Mortar.
- **Dartling: `first_global`.** Zielt auf den vordersten anvisierbaren Bloon der Karte, mit der Drehrate aus
  `RotateToPointerModel` (180 °/s). Das entspricht einem Spieler, der die Maus auf die Spitze hält.
  Bridge: nicht über Bridge-Aktionen (die Warteschlange hätte einen Schritt Versatz, wie die Probe gezeigt hat),
  sondern die Mod setzt den "Locked"-Punkt im `Simulate`-Prefix je Schritt mit derselben Regel wie die Sim
  [prüfen im Player].
- Alternative für später (nicht in diesem Schritt): eine Aktion "Zielpunkt setzen" mit N Zellen, die für den
  zuletzt gebauten oder upgegradeten Punkt-Turm gilt (+720 Aktionen, +1 Kanal "Zielpunkte").
- Heli und Ace: Modus fest (Lock bzw. Pursuit ab 2xx; Acht um die Startbahn), keine Aktion.
- Spike Factory: Modus fest ("Normal"/`track_spread`), auch nach Directed Spikes.

## 4. Konverter und Daten

- `--towers t21`: die 14 aus Schritt 2, dann `alchemist`, `engineer`, `mortar`, `dartling`, `spike`, `heli`, `ace`.
  Reihenfolge fest, damit das Layout stabil bleibt.
- **Alle sieben sofort in Daten und Layout** (wie die Farm in Schritt 2). Türme, deren Mechanik noch fehlt, stehen in
  `MECHANIC_LOCKED` und damit in allen Presets auf `locked_towers`. Jeder Teilschritt hebt nur Sperren auf; Kanäle
  und Aktionsraum ändern sich danach nicht mehr. Das setzt voraus, dass zu jedem gesperrten Turm schon die
  Basisvariante konvertierbar ist. Deshalb schreibt der Konverter für einen gesperrten Turm, dessen Basis noch
  nicht geht, einen Platzhalter (Basis mit leerem Angriff, `income`/`supports` leer, aber mit dem echten Footprint inkl.
  Rechteck bei Heli 27 × 27 und Ace 28 × 18, damit sich Maske und Kollision später nicht ändern), den die Validierung nur bei
  gesperrten Türmen zulässt [Alternative: Turm erst aufnehmen, wenn seine Basis geht; dann ändert sich das Layout
  mehrmals].
- Neue Modelle wie bisher STRICT (nur an der ausgewerteten Stelle erlaubt) mit Feld-Prüfung gegen den Dump-Standard.
- Neue Felder (`lob`, neue `targeting`-Varianten, `ground`, `timed`/Tränke, `sub_towers`, `cleanse`, `unit`; `cash_on_pop` erst nach Klärung, §2.6) sind optional;
  primary-, t12- und t14-Daten bleiben unverändert (außer `data_hash`, falls sich Standardfelder ändern – vermeiden).
- Abdeckungs-Report wie bisher je Turm, mit Hinweis auf die Vereinfachungen (Zielpunkt-Regel, Mittelwert-Streuung,
  Long Life, Flieger-Modi).

## 5. Reihenfolge und Priorität für CHIMPS

Einschätzung des CHIMPS-Nutzens (Monkey Meadow, 1 Leben, Einkommen nur aus Pops und Rundenbonus `end_bonus`):
- **Alchemist** (Brew/Stronger Stimulant auf starke Primary-/Magic-Türme, Säure gegen Lead und MOAB-DoT) – hoch.
- **Spike Factory** (Absicherung am Streckenende gegen Leaks, MOAB SHREDR, Spiked Mines) – hoch; bei 1 Leben
  besonders wertvoll.
- **Mortar** (billig gegen Lead/Gruppen, Signal Flare und Shattering Shells gegen Camo/Fortified) – mittel, aber
  mit wenig Mechanik viel Abdeckung (29 schon mit Zielpunkt).
- **Engineer** (Schaum gegen Camo/Regrow, Sentries) – mittel; der Hauptnutzen in CHIMPS, Overclock, ist eine Ability.
- **Dartling** – mittel (spät stark: BADS, Ray of Doom).
- **Heli, Ace** – niedrig bis mittel, Flieger-Mechanik am teuersten.

Teilschritte (jeder mit Tests und Parität; nach Teil 2 ein t21-Lauf):

| Teil | Inhalt | freigeschaltet (Obergrenze) |
|---|---|---|
| 0 | Konverter/btd-data t21 (alle 7 im Layout, gesperrt), Validierung, Env-Layout; Engineer-Basis frei; **Place-Maske steigt für gesperrte Türme früh aus** (nur Nullen, keine Kollisionsprüfung), damit Heli/Ace keine Rechenzeit kosten, bevor sie etwas tun | Engineer 6 |
| 1a | B1 Wurf + `track_best`; Pin, Double Gun (ohne Zeit-Effekte) | Mortar 29 (Signal-Flare-Varianten erst mit Teil 2), Engineer 12 |
| 1b | B4 Zeit-Effekte + `friendly` (größtes Paritätsrisiko: effektive Werte ändern sich im Tick-Loop, deshalb allein) | Alchemist 18 (Perishing-Potions-Varianten 020/021/120/220 erst mit Teil 2) |
| 2 | B3 Bodenobjekte + `track_spread` + `track_nearest` (Schaum) + B6 Modifier entfernen (ohne Cash je Pop), dazu Spike S2 (Bonus, Minen) und S3 (Directed) | Spike 49, Alchemist 29 (ohne Lead/Rubber to Gold, §2.6), Engineer 15 (Schaum), Mortar alle 29 |
| – | **Lauf t21** (R60 → CHIMPS) mit Alchemist, Spike, Mortar, Engineer | |
| 3 | B5 Sub-Türme | Engineer 39 (Sentries; 44 erst mit der Falle E4, später) |
| 4 | Dartling (`first_global`, D1–D3), Mortar M2, Alchemist A4 (A3 nur, falls Cash je Pop freigegeben wird), Spike S4 (Super Mines) | Dartling 44, Mortar 49 |
| 5 | B7 Flieger – ob überhaupt, wird nach dem t21-Lauf entschieden (Heli/Ace bleiben im Layout) | Ace 24–42, Heli 14–29 |

## 6. Determinismus und Parität

- Ohne die neuen Felder in den Daten läuft alles wie bisher: sim_v0-Goldtrace (f64) und vorhandene Replays (t12,
  t14) bleiben bitgleich. Neue Listen (`ground`, `subs`) sind leer, `unit` = Turmposition, `targeting` = `first`.
- **Zeitlicher Zustand nur innerhalb der Runde.** Zeit-Effekte, Bodenobjekte, Sub-Türme, Flieger-Positionen,
  Zähler (`track_spread`, Sentry-Index, Sentry-Kandidat) werden zu Rundenbeginn zurückgesetzt, wie die Cooldowns.
  Zwischen den Runden hängt der Zustand also weiter nur von Cash, Leben, Runde und Türmen ab:
  `state_hash`, `StartState`, Replays und Observation brauchen nichts Neues.
  - Folge: Spikes, Sentries und Brews, die im Spiel über das Rundenende hinaus liegen bzw. laufen, fehlen in der
    Sim zu Beginn der nächsten Runde. Das macht die Sim etwas schwächer als das Spiel (eher sicher als zu
    optimistisch). Betroffen sind vor allem Long Life Spikes (xx3+) und Sentries am Rundenende [KAL].
  - Spätere Option: Bodenobjekte über das Rundenende behalten (dann in Hash, `StartState` und Replay).
- Neue Zufallsquellen (Spike-Punkt, Streuung, Sentry-Position) nur mit Seed (`randomness`), sonst feste
  Mittelwert-Regeln. Reihenfolge der Würfe fest (Turm-ID, Angriff, Erzeugung).
- Rechnung in `Real` wie der Rest des Tick-Loops; Vorab-Tabellen (Strecken-Intervalle, Sentry-Kandidaten, Ace-Bahn)
  in f64 berechnet und einmal umgewandelt. Grenzfälle (≤ Reichweite) können sich zwischen f32 und f64 unterscheiden,
  wie heute.
- Zeit-Effekte ändern effektive Werte mitten in der Runde; `refresh_buffs` (Aktionen) und Zeit-Effekte schreiben
  über dieselbe Funktion und dieselbe feste Reihenfolge (erst Auren, dann Zeit-Effekte), damit das Ergebnis nicht
  davon abhängt, welches Ereignis zuletzt kam.

## 7. RL-Env und Observation

- 21 Türme: Kanäle 2 + 21 + 3 + 5 = **31**, Aktionen 1 + (21 + 3 + 1) × 720 = **18 001** (t14: 24 / 12 961).
  Bestehende t14-Modelle passen nicht (neuer Lauf, wie bei t12 → t14).
- **Keine neuen Kanäle.** Alles Neue ist entweder Rundenzustand (zwischen den Runden nicht sichtbar) oder folgt aus
  Turmtyp, Position und Tiers (Mortar-Ziel = fester Punkt je Karte). Brew/Dip wirken nur während der Runde; die
  Policy sieht, wo ein Alchemist mit welchen Tiers steht. Falls Alchemisten selten gebaut werden: optionaler
  Kanal "in Reichweite eines Brew-Alchemisten" (wie die Buff-Kanäle) [Entscheidung nach dem Lauf].
- Zufallssperren: höchstens ⌊25 % der **freien** Türme⌋ (Regel statt fester Zahl, gilt in jedem Teilschritt).
  Frei = nicht fest gesperrt; fest gesperrt sind die Farm (CHIMPS bzw. bis zur Mechanik) und Türme ohne Mechanik.
  Beispiel nach Teil 2: 21 − Farm, Dartling, Heli, Ace = 17 frei → höchstens 4. `--ban-towers` über der Grenze
  wird abgelehnt.
- Leistung: Die Place-Maske wächst mit den Turmtypen (t12 → t14: +11 % je Schritt). Erwartung für t21 grob +30 %
  Schrittzeit gegenüber t14, sobald alle Türme frei sind; gesperrte Türme kosten dank der früh aussteigenden Maske
  fast nichts; dazu Kosten im Tick-Loop für Bodenobjekte (Spike Factory) – messen nach Teil 2.
- `--reward-tower` bestraft auch die neuen Türme (wie die Village).

## 8. Tests und Plausibilität

Rust (btd-core), je Baustein:
- Wurf: landet nach `time` bzw. Abstand/Tempo, trifft unterwegs nichts, Explosion am Zielpunkt, nicht am Bloon
  zum Ankunftszeitpunkt.
- Ziel-Arten: `track_nearest` = nächster Streckenpunkt (Gleichstand → größere Pfad-Distanz); `track_best` =
  Punkt mit maximaler Streckenlänge im Radius (Vergleich mit Brute Force über feines Raster, Gleichstand); `track_spread` deterministisch
  und gleichmäßig über die Strecke in Reichweite; `friendly` wählt nach `spent`, dann ID; `first_global` ignoriert
  die Reichweite, Drehrate begrenzt; Spike-Intervalle folgen der gebufften Reichweite, und läuft ein Brew mitten
  in der Runde ab, schrumpfen sie wieder auf die alte Reichweite.
- Bodenobjekte: Pierce-Verbrauch mit Treffer-Reset und Kollisions-Intervall, halten die Runde nicht offen, sind
  nach der Runde weg; Distanz-Fenster gibt dasselbe Ergebnis wie die volle 2D-Prüfung (Test mit Selbstkreuzung).
- Zeit-Effekte: Ablauf nach Zeit und nach Schüssen, Sperrzeit, stärkerer ersetzt schwächeren, zwei Alchemisten
  stapeln nicht, Brew + Village wirken zusammen, Laufzeit-Zustand (Salve, Crit) bleibt erhalten, Reset zu
  Rundenbeginn.
- Sub-Türme: Erscheinen alle n s an deterministischen Positionen, Ablauf, Auren an der Position, nicht im Hash.
- Modifier: Signal Flare macht Camo sichtbar, Kinder erben; Perishing Potions entfernt Fortified, aber nicht bei MOABs.
- Flieger: Ace fliegt die Bahn mit Tempo, Heli-Pursuit erreicht den vordersten Bloon.
- Parität: sim_v0 (f64) und t12/t14-Replays bitgleich nach jedem Teil; `state_hash` zwischen Runden unabhängig von
  Zeit-Effekten und Bodenobjekten (Rundlauf über `StartState`).

btd-data: Validierung der neuen Felder (Wertebereiche, `targeting` nur mit passender Lieferart, Platzhalter nur für
gesperrte Türme), unbekannte Felder werden abgelehnt.

Konverter: Abdeckung je Turm wie in §1 (Obergrenzen), STRICT für alle neuen Modelle, Feld-Prüfungen (z.B.
`RemoveBloonModifiersModel` nur mit bekannten Schaltern), `MECHANIC_LOCKED` je Teil; `GiveCashOnPopModel`
bleibt unbekannt, Lead/Rubber to Gold werden also abgelehnt (Test, damit die Sperre nicht versehentlich fällt).

Env: t21-Layout gleich groß, egal welche Türme gesperrt sind; Place-Maske gesperrter Türme ohne
Kollisionsprüfung (Zeitmessung t14 gegen t21 mit gesperrten Türmen); Sperr-Grenze aus den freien Türmen; Maske sperrt Türme ohne Mechanik; Python
`grid_spec` stimmt.

Plausibilität (`btd-tools demo`, Pops je Runde): Dart allein gegen Dart + Brew-Alchemist; Spike Factory allein an
Runde 30–40; Mortar mit Ziel auf einer Schleife gegen dieselbe Runde ohne Mortar.

## 9. Kalibrierpunkte für das echte Spiel

Über die Recorder-Mod (effektive Werte nach Aktionen, wie in Schritt 2 §5) bzw. kurze Testpartien:
1. Dip-Ladungen: `cap` 30 oder `maxCount` 10 Schüsse; was zählt als "Angriff" bei Brew-Schüssen (je Waffe?).
2. Brew-Zielwahl im Spiel; Tempo-Formel (÷ 1,15 oder × 0,85); Verrechnung mit Village-Tempo.
3. Spike-Factory-Punktwahl ("Normal"): Verteilung über die Strecke in Reichweite; Flugzeit bis "liegt".
4. Feuern Spike Factory/Mortar/Dartling ohne Bloons in Reichweite bzw. auf der Karte?
5. Laufen Brew, Sentries, Spikes über das Rundenende (Spieluhr zwischen Runden mit sofortigem Start)?
6. Sentry-Positionen (`idealDistanceWithinTrack`) gegen die Kandidatenregel.
7. Cleansing Foam/Signal Flare: Was heißt `cleanseLead` genau?
8. Pin (`SlowOnPopModel` "Stun:Weak"): Dauer und welche Bloons.
9. Ace-Acht: Radius, Startpunkt und Richtung; Heli-Pursuit-Abstand.
10. Mortar-Streuung und ob die 0,5 s ab Abschuss oder ab Zielwahl zählen.

## 10. Offene Fragen (beantwortet, siehe §11)

1. **Layout:** Alle sieben Türme sofort ins Layout (t21, gesperrt bis zur Mechanik, mit Platzhaltern für noch nicht
   konvertierbare Basisvarianten), damit sich der Aktionsraum nur einmal ändert (Vorschlag) – oder schrittweise
   (t17 mit Alchemist/Spike/Mortar/Engineer, später t21)?
2. **Zielpunkte:** Mortar = nächster Streckenpunkt (über den Bauplatz gewählt) und Dartling = automatisch auf den
   vordersten Bloon (Vorschlag) – ok? Kann die Bridge den Dartling-Punkt laufend nachführen, oder soll Dartling
   lieber auch einen festen Punkt bekommen? Eine echte Ziel-Aktion (+720 Aktionen) erst später?
3. **Rundenzustand zurücksetzen:** Tränke, Spikes, Sentries und Flieger-Positionen zu Rundenbeginn löschen (kein
   neuer Zustand in Hash/Startzuständen/Observation; Sim etwas schwächer als das Spiel, v.a. Long Life Spikes) – ok?
4. **Abilities:** weiter gesperrt (Overclock, Spike Storm, Artillery Battery, Transforming Tonic, …)? Overclock ist für
   CHIMPS wichtig; wäre ein eigener Ability-Schritt danach sinnvoll?
5. **Reihenfolge:** Alchemist + Mortar (Teil 1), dann Spike Factory (Teil 2), dann Lauf, dann Sentries, Dartling,
   Flieger. Heli und Ace ans Ende – oder ganz weglassen, bis CHIMPS mit den übrigen läuft?
6. **Zufallssperren:** bei 21 Türmen bis 25 % = 5, oder bei 3 bleiben?
7. **Brew-Zielwahl:** Turm mit dem höchsten `spent` in Reichweite (Vorschlag), bis der Recorder die Spielregel zeigt?
8. **Kleinere Festlegungen:** Spike Factory feuert nur bei anvisierbarem Bloon (wie im Dump); Heli-Pursuit hält
   `pursuitDistance` 32; Ace fliegt immer die ungedrehte Acht (auch ab xx2) – ok?

## 11. Freigabe und Entscheidungen (2026-10-27)

Der User hat das Design mit diesen Anpassungen freigegeben (eingearbeitet in §2–§8):
- Zufallssperren als Regel: höchstens 25 % der freien Türme (§7).
- Mortar zielt auf `track_best` (fester bester Punkt je Karte und Radius) statt `track_nearest` (§2.2, §3).
- Lead/Rubber to Gold: Ob CHIMPS sie zulässt, zeigt der Dump nicht; vermutlich gesperrt. B6 baut zuerst nur das
  Entfernen der Modifier, Cash je Pop wartet (§2.6). §5 nennt jetzt auch den Rundenbonus als Einkommen.
- Teil 1 geteilt: 1a Mortar (B1 + `track_best`), 1b Alchemist (B4 + `friendly`), damit sich ein Paritätsbruch eindeutig
  zuordnen lässt (§5).
- Teil 0: Place-Maske steigt für gesperrte Türme früh aus (§5, §8).
- Test: Brew läuft mitten in der Runde ab, `track_spread`-Intervalle schrumpfen wieder (§8).

Antworten auf §10:
1. Alle sieben sofort ins Layout (t21, Platzhalter), unter der Bedingung der früh aussteigenden Maske.
2. Mortar: `track_best`. Dartling `first_global` in der Sim ok; in der Bridge setzt die Mod den "Locked"-Punkt im
   `Simulate`-Prefix je Schritt mit derselben Regel [prüfen im Player]. Echte Ziel-Aktion später, wenn überhaupt.
3. Reset zu Rundenbeginn ok (Sim eher zu schwach als zu stark ist die richtige Fehlerrichtung).
4. Abilities in diesem Schritt gesperrt. Eigener Ability-Schritt danach lohnt sich (Overclock); billige erste Version
   "Auto-Cast, sobald bereit" ohne neue Aktion. Ability-Cooldowns laufen im Spiel über das Rundenende, das wäre der
   erste Rundenzustand in Hash und `StartState`.
5. Reihenfolge ok mit Teil 1a/1b. Heli und Ace bleiben im Layout; ob Teil 5 kommt, wird nach dem t21-Lauf entschieden.
6. Zufallssperren: Prozentsatz der freien Türme (siehe oben).
7. Brew-Ziel nach höchstem `spent` als Platzhalter; der Recorder soll die echte Regel früh zeigen, weil die Policy um
   die Heuristik herum lernt.
8. Spike Factory nur mit anvisierbarem Bloon, Heli-Pursuit mit `pursuitDistance` 32, Ace immer die ungedrehte Acht: ok.

## Stand Teil 0 (Daten und Layout, 2026-10-27)

- **Konverter:** `--towers t21` (die 14 aus t14, dann `alchemist`, `engineer`, `mortar`, `dartling`, `spike`, `heli`,
  `ace`): 354 von 1344 Kombinationen, also t14 (348) plus Engineer 6 (000, 001, 010, 011, 020, 021). Engineer ist frei.
  - Alchemist, Mortar, Dartling, Spike, Heli und Ace stehen in `MECHANIC_LOCKED` und damit in allen Presets auf
    `locked_towers`.
  - Ihre Basisvariante geht noch nicht. Deshalb steht je ein Platzhalter in den Daten (`placeholder`): echte
    Reichweite und echter Footprint (Heli und Ace als Außenkreis des Rechtecks), keine Wirkung. Der
    Abdeckungs-Report nennt die Platzhalter.
  - Das Baubar-Raster ist unverändert. Der größte runde Footprint bleibt 11, Rechtecke zählen nicht mit.
  - Die t14-Ausgaben (medium60, chimps, Daten und Abdeckungs-Report) sind bytegleich zu den bisherigen Dateien.
- **btd-data:** Eine Variante ganz ohne Wirkung ist nur als Basisvariante eines Turms in `rules.locked_towers`
  erlaubt. Ein
  Formatfeld kommt nicht dazu.
- **Env:** Kein neuer Code nötig.
  - Die Place-Maske überspringt nicht verfügbare Türme schon (`unavailable` → `continue`), ohne Kollisionsprüfung.
  - `map_ok` wird einmal in `Env::new` für alle Türme berechnet.
  - Neuer Test `placeholder_tower_is_in_the_layout_but_never_placeable`: Der Platzhalter steht im Layout, ist nie
    in der Maske und wird nie als Zufallssperre gezogen. Bauen gibt `TowerLocked`. Die Maske der übrigen Türme ist
    dieselbe wie ohne ihn.
- **Zufallssperren:** `train.py` lehnt `--ban-towers` über ⌊25 % der freien Türme⌋ ab (`max_bans`). Die Env selbst
  verlangt weiter nur weniger Sperren als freie Türme; so bleiben alte Tests und Modelle ladbar.
  - t21 hat heute 14 freie Türme, also höchstens 3, wie t14.
  - Mit sim_v0 ist höchstens 1 erlaubt. Ältere Befehle mit 2 Sperren auf sim_v0 lehnt `train.py` jetzt ab.
- **Layout t21:** 31 Kanäle, 26 Skalare ohne Vorschau, 18 001 Aktionen. Python `grid_spec` stimmt damit überein.
- **Gemessen** (`btd-tools bench`, Release, eine Env, Zufallspolicy; verrauscht, weil parallel ein Training lief):
  - Masken/s: t14 69–71 k, t21 55–65 k.
  - Env-Schritte/s: t14 5,9–6,0 k, t21 5,0–5,8 k.
  - Die Mehrkosten kommen von den größeren Masken- und Observation-Puffern, nicht von den gesperrten Türmen.
- **Offen:**
  - Die installierte `btd_rs` in `.venv` lädt t21 nicht: Ihre Validierung lehnt die Platzhalter ab. Sie wird
    nach dem laufenden t14-Training aus diesem Stand neu gebaut.
  - Die Datendateien liegen bisher nur im Worktree unter `data/local/`.
  - `random_build` (zufällige Startzustände, Quelle C) zieht den Turmtyp gleichverteilt über alle Türme; ein Wurf
    auf einen gesperrten Typ verfällt. Bei t21 sind das 7 von 21 (bei t14 1 von 14), zufällige Startzustände
    haben also weniger Türme. Bewusst nicht in Teil 0 geändert, weil eine Ziehung nur unter freien Typen den
    Zufallsstrom für t14 verschiebt; vor dem t21-Lauf entscheiden.

## Stand Teil 1a (Mortar, Engineer Pin/Double Gun, 2026-10-27)

- **btd-data:**
  - Neue Lieferart `Delivery::Lob {time | speed, spread, on_arrive}`.
  - `Targeting::TrackBest`, nur mit Wurf und Explosion; der Radius ist die größte Explosion des Wurfs
    (`Delivery::track_best_radius`).
  - Validierung: genau eins von `time`/`speed`, `on_arrive` nicht leer, nur Explosionen.
- **Engine:**
  - Ein Wurf ist ein Projektil ohne Kollision. Es fliegt in ganzen Ticks (`ticks_of`) vom Turm zum Zielpunkt
    und löst dort `on_exhaust` aus.
  - Zielpunkt ist der vorderste Bloon beim Abwurf oder bei `track_best` der feste Punkt.
  - Gefeuert wird wie bisher nur, wenn ein anvisierbarer Bloon in Reichweite ist; beim Mortar ist das die
    ganze Karte.
  - `map::best_track_point` rastert die Strecke in Schritten von 1 und nimmt nur Punkte innerhalb der
    `bounds`. Gewertet wird die Streckenlänge im Kreis, als exakter Schnitt Segment/Kreis. Gleichstand (bis
    auf 1e-9) entscheidet die größere Pfad-Distanz.
  - Die Punkte rechnet `Game::new` einmal je Radius in f64 aus (`Game::track_best_points`).
  - Streuung nur mit `randomness` (gleichverteilt im Kreis aus `Game::rng`), sonst genau der Punkt.
- **Konverter:**
  - Erkennungsmerkmal ist `TargetSelectedPointModel`; ausgewertet wird es in `convert_point_lob` (Mortar).
    Die Granate hat `InstantModel` + `AgeModel` (Flugzeit), `FilterAll` und keinen eigenen Schaden.
  - Explosionen bei der Ankunft (`CreateProjectileOnExhaustFraction`) werden zu `on_arrive`. Das Streuen
    (`RandomTargetSpreadModel`) wird zu `spread`.
  - Die neuen Modelle sind STRICT. `maxDamage` ab 10⁶ gilt als unbegrenzt (Mortar-Explosion 9 999 999).
  - `EmissionWithOffsetsModel`: ein Projektil je Schuss wird `single` (Double Gun wechselt nur den
    Abwurfpunkt). Je Abwurfpunkt eins, symmetrisch angeordnet, wird `parallel`.
  - Abwurfpunkte, das Kartenobjekt `ChipMapBasedObjectModel` und Pin (`SlowOnPopModel`) werden ignoriert.
- **Abdeckung t21:** 374 von 1344 Kombinationen.
  - Mortar **14** statt der hochgerechneten 29, jetzt frei (nicht mehr in `MECHANIC_LOCKED`).
  - Engineer **12**.
  - t14-Daten bleiben bytegleich, im t14-Report ändern sich nur Begründungstexte.
- **Abweichungen vom Plan:**
  - Burny Stuff (Mortar xx2, 15 Kombinationen bis xx4) braucht einen erweiterten DoT, den die Engine noch
    nicht kann: Feuer-DoT mit Tag-Filter (nicht DDT/Black/Zebra), "nur bei Schaden" und eigener Immunität
    (Purple). Damit hängen auch Signal Flare (xx3) und Shattering Shells (xx4) daran. Vorschlag: zusammen mit
    B6 in Teil 2.
  - Pin (`SlowOnPopModel`: 1 s Stopp der Kinder beim Pop, Feld `Lifespan`) bleibt ohne Wirkung, weil die
    Engine noch keinen Status-Effekt beim Pop setzen kann [KAL: auch `chance` 0 – immer oder nie?]. Die Sim ist
    dadurch eher zu schwach. Nachholen mit B6 in Teil 2.
  - Wurf-Angriffe haben selbst weder Schaden noch Crit (Validierung); beides steckt in den Explosionen.
  - Für Teil 1b merken: Der Village-Tempo-Buff wirkt auf `def.speed`, nicht auf `LobRt.speed`. Für den Mortar
    (feste Zeit) egal, beim Alchemisten (Tempo) zu entscheiden.
  - Double Gun ist kein zweiter Nagel. Das Tempo kommt aus der Rate der Variante.
- **Tests:**
  - `btd-core/tests/lob.rs`: Flugzeit (fest und über Tempo), Explosion am Punkt statt am Ziel bei der Ankunft,
    `track_best` von überall, Streuung nur mit Zufall und deterministisch je Seed, ungültige Würfe.
  - `map.rs`: Punktwahl in einer Haarnadel, Gleichstand, `bounds`, Schnittlänge.
  - Ein lokaler Test gegen die t21-Daten (`dump_local`, `--ignored`) zeigt die Zielpunkte auf Monkey Meadow:
    Radius 20 bei (−50,8; −19,0), Radius 28 bei (−50,8; −2,0). Ein Mortar in einer Ecke platzt dort alle 20
    Reds von Runde 1.
  - Konverter-Tests für Mortar und `EmissionWithOffsetsModel`.
  - sim_v0-Parität (f64) und alle übrigen Tests unverändert grün.
- **Gemessen** (Release, eine Env, Zufallspolicy, verrauscht): t14 etwa 6,0 k Schritte/s, t21 mit Mortar und
  Engineer etwa 5,7 k Schritte/s.

## Stand Teil 1b (Alchemist, Zeit-Effekte, 2026-10-27)

- **btd-data:**
  - `Targeting::Friendly` und `Delivery::Potion {effect}` mit `TimedEffect`. Felder: Gruppe, Rang, Sekunden,
    Schüsse, Sperre, Schaden, Pierce, Tempo- und Reichweiten-Anteil, Bonus-Schaden, ausgeschlossene Türme.
  - `Bonus.all`: Tags, die der Bloon zusätzlich alle haben muss ("Lead,Fortified").
  - Validierung: `friendly` nur mit Trank, Wurf und Trank ohne eigenen Schaden oder Crit, Wertebereiche,
    bekannte Tags und Turmnamen.
- **Engine:**
  - `Tower::timed` und `Tower::blocked` (je Gruppe höchstens ein Effekt). `Tower::rebuild` rechnet die
    wirksamen Werte aus Basis, Auren und Zeit-Effekten; ohne Zeit-Effekte bitgleich wie bisher.
  - Reichweiten-Anteile addieren sich, das Tempo teilt den Cooldown [KAL]. Pierce gilt nur für fliegende
    Projektile. Schaden und Bonus-Schaden gehen nur an Angriffe und Kontakte, deren Schaden > 0 ist; Glue
    z.B. bleibt ohne Schaden (Test).
  - Ein Trank wählt unter den Türmen in Reichweite den mit dem höchsten `spent` (Gleichstand: kleinste ID).
    Der Turm muss einen Angriff haben, darf nicht ausgeschlossen oder für die Gruppe gesperrt sein und
    keinen gleich starken oder stärkeren Effekt der Gruppe tragen.
  - Ein höherer Rang ersetzt den schwächeren Effekt. Tränke fliegen nicht: Der Effekt wirkt sofort, die
    Flugzeit liegt unter 0,2 s [KAL].
  - Ablauf je Tick (Restzeit) und je Schuss irgendeines Angriffs (Rest-Schüsse); danach `block` Sekunden
    gesperrt.
  - Tränke werden nur geworfen, solange Bloons auf der Karte sind; die Restzeit läuft auch in Spawn-Lücken
    ab [KAL: im Spiel wirft der Alchemist wohl auch dann]. Die Sim ist dadurch eher zu schwach.
  - Ein Trank-Angriff hat weder Schaden noch Crit, Salve, Camo oder Extras (Validierung).
  - `play_round` löscht Zeit-Effekte und Sperren am Anfang und am Ende der Runde. Zwischen den Runden ist der
    Zustand also wie bisher (`state_hash`, Observation, Startzustände unverändert).
  - Bonus-Schaden prüft jetzt zusätzlich die `all`-Maske; bei leerer Maske wie bisher.
- **Konverter:**
  - Der Alchemist-Wurf (`ArriveAtTargetModel` mit `altSpeed`) wird zu `lob` mit Tempo und dem Säure-Spritzer
    als Explosion (Säure-DoT wie beim Glue).
  - Dip (`AddAcidicMixtureToProjectileModel`) und Brew (`AddBerserkerBrewToProjectileModel`) werden zu
    `potion`.
    - Gruppe beim Brew ist der erste Eintrag von `ignoreMutationsByOrder`, der Rang die Position der eigenen
      `mutationId` darin (3xx 0, 4xx 1).
    - Dip: `cap` (30) als Schüsse [KAL: `maxCount` 10], Bonus +1 gegen Ceramic, MOABs, Lead + Fortified.
    - Ausgeschlossen sind die Türme aus den Ignorier-Listen von Trank und Zielwahl, gekürzt auf das Turm-Set
      (in t21: alchemist, farm, village).
    - Annahmen werden geprüft: Der Brew ersetzt genau die schwächeren Stufen (`mutatorsToRemoveList`), Zähler
      und Mutationen passen zueinander, Dauer und Sperre sind endlich (Permanent Brew wird abgelehnt), und die
      Zielwahl ist nicht wählbar.
  - `AcidPoolModel` (Pfütze nur beim Fehlwurf, 0,05 s) ist ignoriert [KAL].
  - `CanBuffIndicatorModel` ist nur als Anzeige der Tränke ausgewertet. Super Monkey 3xx nutzt dasselbe
    Modell für den Tempel und bleibt gesperrt; als Anzeige würde es ihn freigeben und t14 ändern.
- **Abdeckung t21:** 390 von 1344.
  - Alchemist **16**, frei: Pfade oben 0–4, Mitte 0–1, unten 0–1.
  - Es fehlen Perishing Potions (x2x, Bonus mit Multiplikator/Fortified entfernen), Acid Pool und Gold
    (xx2+), Unstable Concoction, Abilities und Permanent Brew.
  - t14-Daten bleiben bytegleich. Im t14-Report ändern sich nur Begründungstexte: Super Monkey 3xx nennt
    jetzt `CanBuffIndicatorModel (an dieser Stelle)`.
- **Tests:**
  - `btd-core/tests/potion.rs`: Brew-Schaden platzt einen Ceramic, Schuss-Grenze, keine Stapelung je Gruppe,
    höherer Rang ersetzt, Ausschluss, Sperre, Zielwahl nach `spent`, Ende mit der Runde, Validierung.
  - Konverter-Tests für Wurf, Brew-Rang und Bonus mit allen Tags.
  - Lokal gegen die t21-Daten (`dump_local`): Ein Dart mit Alchemist 4-0-1 direkt daneben lässt in 20 Runden
    68 statt 136 Leben durch. Der Vergleich ist Alchemist 1-0-1: gleiche Spritzer, keine Tränke.
  - sim_v0-Parität und alle übrigen Tests unverändert grün.
- **Gemessen** (Release, eine Env, Zufallspolicy, verrauscht durch das parallele Training): t14 6,0–6,1 k
  Schritte/s, t21 mit Alchemist, Mortar und Engineer 5,4–5,7 k.
- **Offen bzw. für später:**
  - Der Village-Tempo-Buff wirkt nicht auf das Tempo des Alchemist-Wurfs. Alchemist ist "magic" und bekommt
    Primary Training ohnehin nicht.
  - Brew gilt nicht für Sub-Türme (kommen erst in Teil 3).

## Stand Teil 2a (Bodenobjekte, Spike Factory, 2026-10-27)

- **btd-data:**
  - `Delivery::Lob` hat jetzt ein optionales `ground`. Felder: Lebensdauer ab der Landung, Radius, Pierce,
    Schaden, Schadensarten, Camo, Extras, Treffer-Reset, Kollisions-Intervall, Explosionen beim
    Verbrauch/Ablauf.
  - `Targeting::TrackSpread`, nur mit Wurf.
  - `VariantV1.round_start`: Zeit-Effekt auf sich selbst zu Rundenbeginn.
  - `TimedEffect.shots` ist jetzt optional (ohne Angabe unbegrenzt).
- **Engine:**
  - **Bodenobjekte:** Ein gelandeter Wurf mit `ground` wird ein liegendes Projektil. Abweichend von §2.3 gibt
    es keine eigene Liste `Game::ground`: Die Objekte liegen mit Flag `ground` in `projectiles`, in
    Erzeugungsreihenfolge (deterministisch).
    - Es prüft Kollisionen nur alle `collide_every` Ticks, die erste gleich nach der Landung.
    - Treffer-Reset wie bei fliegenden Projektilen. Nach Verbrauch oder Ablauf löst es `on_exhaust` aus.
    - Liegende Objekte halten die Runde nicht offen und verschwinden am Rundenende.
    - Auren und Zeit-Effekte wirken auch auf sie: Camo, MIB, Schaden, Bonus-Schaden und Pierce (Pierce-Aura
      bzw. Brew-Pierce auf den Stapel [KAL]).
    - Die 1D-Vorfilterung über die Pfad-Distanz (§2.3) ist noch nicht gebaut. Die Kollision prüft alle Bloons
      jedes `collide_every`-te Tick (Spike: 5 Ticks); messbar langsamer ist das bisher nicht.
  - **`track_spread`:** Gefeuert wird, solange ein anvisierbarer Bloon auf der Karte ist (Camo-Regel des
    Angriffs). Ohne Strecke in Reichweite kein Wurf.
    - Zielpunkt: `map::track_point_in_circle` (Stücke der Strecke im Kreis in Pfad-Reihenfolge, f64) beim
      Anteil frac(k·0,618…). k zählt die Würfe der Runde je Angriff.
    - Mit `randomness` ist der Anteil gleichverteilt. Der Querversatz `maxOffset` entfällt [KAL].
    - Der Radius ist die wirksame Reichweite (`AttackRt::range`, mit Auren und Brew).
  - **Rundenstart-Effekt:** `play_round` gibt ihn nach dem Löschen der Zeit-Effekte dem eigenen Turm.
- **Konverter (`convert_track_lob`, `convert_round_start`):**
  - `TargetTrackModel` (Spike Factory) wird zu `track_spread`, und `ArriveAtTargetModel` mit `timeToTake` und
    `stopOnTargetReached` zu `lob.time` mit `ground`.
  - Lebensdauer des Stapels = `AgeModel.lifespan` (Rundenzeit) minus Flugzeit. `checkCollisionInterval` wird
    zu `collide_every`.
  - `DamageModifierForBloonTypeModel` (Spiked Balls: +3 gegen Ceramic) wird als Tag-Bonus übernommen.
  - Ignoriert werden Darstellung und Modus: `Fade…`, `RandomRotation…`, `EndOfRoundClearBypass` (nur
    Apopalypse), `ExpireProjectileOnBossSpawned` (keine Bosse) und die Modi von Directed Spikes
    (Smart/Auto/Close/Punkt). Die Sim bleibt bei der Verteilung über die Strecke [KAL].
  - Long Life und Deadly Spikes (`AgeModel.rounds` 1/2) überdauern das Rundenende nicht (§6). Die längere
    Lebensdauer (140 s) und der höhere Schaden gelten.
  - Perma-Spike (xx5, `rounds` 3, ein Wurf alle 6 s) ist gesperrt: Seine Stärke liegt im Liegenbleiben über Runden, ohne das wäre
    er in der Sim stark entwertet.
  - Fliegende Würfe halten die Runde nach dem letzten Bloon noch bis zur Landung offen (höchstens 0,9 s;
    ändert nur `seconds`).
  - `StartOfRoundRateBuffModel` wird zu `round_start`; `modifier` 0,25 gilt als Cooldown-Faktor, also 4-fach
    schneller für `duration` Sekunden [KAL].
  - Spiked Mines (Explosion beim Verbrauch und beim Ablauf, beide gleich) sind vorbereitet. Sie scheitern noch
    am Feuer-DoT mit Purple-Immunität (Teil 2b).
- **Abdeckung t21:** 429 von 1344.
  - Spike Factory **39**, frei. Es fehlen Spiked Mines (4xx, Teil 2b), Super Mines (5xx), Perma-Spike (xx5),
    Spike Storm/Carpet (Abilities).
  - t14-Daten bleiben bytegleich.
- **Tests:**
  - `btd-core/tests/ground.rs`: Pierce-Verbrauch, Lebensdauer, keine Strecke in Reichweite, Pierce-Zuschlag und
    Explosion beim Verbrauch am Stapel, Rundenende trotz
    liegender Spikes, Kollisions-Intervall und Treffer-Reset, Verteilung und Determinismus (auch mit Seed),
    Rundenstart-Effekt, Validierung.
  - `map.rs`: Punkt im Kreis in Pfad-Reihenfolge.
  - Konverter-Test für Spike Factory, Spiked Balls und Directed Spikes.
  - Lokal gegen die t21-Daten: Eine Spike Factory allein lässt in den Runden 1–10 43 statt 738 Leben durch.
- **Gemessen** (verrauscht, Training läuft parallel): t21 4,3–5,5 k Schritte/s gegen t14 5,8 k.

## Stand Teil 2b (erweiterter DoT, Camo entfernen, Pin, Spiked Mines, 2026-10-27)

- **btd-data:**
  - `Dot` hat drei neue Felder: `cannot_damage` (Feuer trifft kein Purple), `only_if_damaged` und
    `exclude_tags`.
  - `HitExtras.cleanse`: Camo entfernen, wahlweise nur bei Schaden, nicht bei Bloons mit Tag.
  - `BloonDefV1.decamo`: derselbe Bloon ohne Camo. In den Spiel-Daten ist Camo ein eigener Typ (`RedCamo`),
    kein Instanz-Flag.
  - `HitExtras.pop_slow`: Verlangsamen der Kinder beim Platzen, nur ohne Glue-Stufe.
  - Alle Felder sind optional, ältere Daten laden und rechnen unverändert.
- **Engine:**
  - Ein DoT wirkt nur bei Schaden (falls verlangt) und nicht bei ausgeschlossenen Tags. Die Immunität reist
    mit dem Bloon (`BloonStore::dot_cannot`) und geht an die Kinder.
  - `cleanse` wechselt vor dem Schaden auf den Typ ohne Camo (gleiche Lebenspunkte) und löscht das
    Instanz-Flag. Die Kinder kommen dann vom Typ ohne Camo. Mit "nur bei Schaden" gilt es nach einem Treffer
    mit Schaden für den Bloon bzw. seine Kinder.
  - Unter MIB (Village) fallen die Tag-Ausschlüsse des DoT weg: Burny Stuff brennt dann auch DDT, Black und
    Zebra. Im Dump schaltet MIB genau diese Filter ab; DoT-Filter ohne diese Aufhebung lehnt der Konverter
    ab.
  - Ein brennender Bloon gibt den DoT an seine Kinder weiter, auch an solche mit ausgeschlossenem Tag (z.B.
    Rainbow an Zebra) [KAL].
  - `pop_slow` setzt bei jedem Kind eines geplatzten Bloons eine Verlangsamung in den Slot seiner ID.
  - `Delivery::track_best_radius` nimmt jetzt die größte Explosion mit Schaden. Die Flare-Explosion von
    Signal Flare (Radius 52, ohne Schaden) verschiebt den Mortar-Zielpunkt also nicht. Die Punkte auf Monkey
    Meadow bleiben (−50,8; −19,0) bzw. (−50,8; −2,0).
- **Konverter:**
  - `convert_dot` wertet `applyOnlyIfDamaged`, `FilterOutTagModel`-Filter und die Immunität des DoT aus.
  - `RemoveBloonModifiersModel` mit Camo wird `cleanse`. Der Konverter nimmt dann zu jedem Camo-Bloon den Typ
    ohne Camo auf (`decamo`, nur in Daten mit einem solchen Turm; t21 chimps hat 63 statt 58 Bloon-Typen).
    Regrow, Lead und Fortified entfernen bleibt gesperrt (Regrow baut main gerade neu; Lead/Fortified wären
    weitere Typwechsel). Das betrifft Shattering Shells, Cleansing Foam und Perishing Potions.
  - `SlowOnPopModel` wird `pop_slow` (1 s Stopp, `chance` 0 gilt als immer, `matchLayersWithDamage` nur bei
    1 Schicht [KAL]).
  - Die Runden-Vorschau der Observation nutzt feste Kategorien. Die zusätzlichen Bloon-Typen ändern das
    Layout also nicht; medium60 und chimps bleiben gleich groß.
  - `DamageModifierForBloonTypeModel` gilt überall, auch in Explosionen.
  - Die Explosion beim Ablauf der Spiked Mines ist gleich der beim Verbrauch und wird mit ihr ausgewertet.
- **Abdeckung t21:** 444 von 1344.
  - Mortar **24** (neu: Burny Stuff, Signal Flare), Spike Factory **44** (neu: Spiked Mines).
  - Engineer weiter 12, jetzt mit wirksamem Pin.
  - t12- und t14-Daten sind bytegleich zur Ausgabe des Konverters aus Teil 2a.
- **Nicht in diesem Teil** (Abweichung von §5, Teil 2):
  - Cleansing Foam (Engineer x3x): braucht Lead entfernen und `track_nearest`.
  - Acid Pool (Alchemist xx2): Säurepfütze als Bodenobjekt mit eigener Zielwahl; die Mechanik ist mit 2a da.
  - Lead/Rubber to Gold bleiben gesperrt (§2.6).
- **Tests:**
  - `btd-core/tests/status.rs`: DoT-Immunität, nur bei Schaden, Tag-Ausschluss und seine Aufhebung unter
    MIB, Camo entfernen am Camo-Typ und am Instanz-Flag (auch nur bei Schaden und mit Ausschluss), Pin hält
    das Kind etwa 1 s, Validierung (auch `decamo`).
  - Konverter-Test für Feuer-DoT, Flare und Pin.
  - sim_v0-Parität und alle übrigen Tests unverändert grün.
- **Rebase auf main mit Regrow (2026-10-27):** Der Branch liegt jetzt auf main (831178a, mit Regrow aus
  docs/design-bloons-chimps.md).
  - Konflikte gab es nur dort, wo beide Seiten Felder ergänzen: `BloonStore` (`grow`/`grow_timer` neben
    `dot_cannot`) und `BloonDefV1` (`grow_rate` neben `decamo`). Beide sind übernommen.
  - Jeder Commit des Branches kompiliert (`cargo check --all-targets`). Alle Tests (f32/f64, sim_v0-Parität,
    Python) und clippy sind grün.
  - Die t14-Dateien des Konverters sind bytegleich zu denen im Haupt-Checkout (main mit `grow_rate`). t21
    hat dieselbe Abdeckung (444/1344) und jetzt Regrow; die lokalen Tests gegen t21 laufen (Alchemist 4-0-1
    neben einem Dart: 72 statt 136 durch, mit Regrow).
  - Camo entfernen und Regrow: Ein entfernter Camo-Typ wächst über seinen Vorfahren-Stapel wieder zu einem
    Camo-Vorfahren nach (z.B. `RedRegrow` zurück zu `BlueRegrowCamo`) [KAL: im Spiel wohl ohne Camo].
