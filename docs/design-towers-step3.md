# Design: mehr Türme, Schritt 3 – Alchemist, Engineer, Mortar, Dartling, Spike Factory, Heli, Ace (Entwurf 2026-10-27, wartet auf Freigabe)

Ziel: von 14 auf 21 Türme. Das sind alle Landtürme, die für CHIMPS auf Monkey Meadow noch fehlen; Wassertürme
(Sub, Buccaneer, Mermonkey) bleiben draußen, Beast Handler und Helden auch. Die sieben Türme brauchen
deutlich mehr neue Mechanik als Schritt 1 und 2. Dieses Dokument ordnet sie deshalb in **sieben gemeinsame
Bausteine** (§2, B1–B7), die jeweils mehrere Türme freischalten, und schlägt eine Reihenfolge nach CHIMPS-Nutzen vor (§5).

Wichtigste Befunde vorab:
- **Engineer 0-0-0 geht schon heute** durch den Konverter (6 Kombinationen ohne neue Engine-Arbeit).
- **Ein gemeinsamer "Wurf"-Baustein** (Projektil fliegt zu einem Punkt, ohne unterwegs zu treffen, und wirkt
  dort) trägt Alchemist, Mortar, Spike Factory, Engineer-Sentries/-Schaum und Ace-Ananas.
- **Zielpunkte ohne neue Aktionsdimension:** Mortar zielt auf den Streckenpunkt, der dem Turm am nächsten
  liegt, der Agent wählt das Ziel also über den Bauplatz; Dartling zielt automatisch auf den vordersten Bloon
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
| xx3 Lead to Gold | `GiveCashOnPopModel` (50 $ je geplatztem Lead, `flatCashAmount`) | Cash je Pop | Cash-Bonus am Treffer (§2.6) |
| xx4 Rubber to Gold | `RemoveBloonTypeModifierModel`, `FilterMutatedTargetModel` | Cash auf alle Bloons | A3 |
| xx5 Bloon Master Alchemist | `MorphBloonModel` | MOABs zu Rot | gesperrt |

Abdeckung: Wurf allein **10**, + Tränke **22**, + Bodenobjekte/Gold **39**, + Unstable **44** von 64.

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

Neue Option an `Delivery::Projectile`: `lob: {time | speed}`. Das Projektil fliegt geradlinig zu einem
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
| `track_nearest` | fester Streckenpunkt, der dem Turm am nächsten liegt; bei Gleichstand der mit der größeren Pfad-Distanz | Mortar (§3), Engineer-Schaum (`startWithClosestTrackPoint`) |
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
  selben Schritt über `Money::from_f64` gutschreiben (mitten in der Runde wird nichts gekauft; paritätssicher)
  [KAL: ob CHIMPS-Mods diesen Bonus zulassen].
- `SlowOnPopModel` (Engineer Pin): Status-Effekt an den Kindern eines geplatzten Bloons, abgebildet mit dem
  vorhandenen Einfrieren/Verlangsamen als kurzer Stopp [KAL: Dauer]; kleiner Teil von B6, kommt aber schon in Teil 1.

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

- **Mortar: `track_nearest`.** Ziel ist der Streckenpunkt, der dem Mortar am nächsten liegt. Der Agent wählt das Ziel
  also über den Bauplatz, Mortar steht dazu ohnehin oft nah an einer guten Stelle. Auf Monkey Meadow liegen viele
  bebaubare Zellen nah an der Strecke. Für die Bridge reicht es, den Zielpunkt einmal beim Bauen zu setzen.
  Nachteil: Ein Mortar weit weg vom Ziel (Spiel-Taktik) ist nicht darstellbar.
- **Dartling: `first_global`.** Zielt auf den vordersten anvisierbaren Bloon der Karte, mit der Drehrate aus
  `RotateToPointerModel` (180 °/s). Das entspricht einem Spieler, der die Maus auf die Spitze hält.
  Die Bridge müsste den Zeiger bzw. den "Locked"-Punkt laufend nachführen (Frage 2).
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
- Neue Felder (`lob`, neue `targeting`-Varianten, `ground`, `timed`/Tränke, `sub_towers`, `cleanse`, `cash_on_pop`, `unit`) sind optional;
  primary-, t12- und t14-Daten bleiben unverändert (außer `data_hash`, falls sich Standardfelder ändern – vermeiden).
- Abdeckungs-Report wie bisher je Turm, mit Hinweis auf die Vereinfachungen (Zielpunkt-Regel, Mittelwert-Streuung,
  Long Life, Flieger-Modi).

## 5. Reihenfolge und Priorität für CHIMPS

Einschätzung des CHIMPS-Nutzens (Monkey Meadow, 1 Leben, kein Einkommen außer Pops):
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
| 0 | Konverter/btd-data t21 (alle 7 im Layout, gesperrt), Validierung, Env-Layout; Engineer-Basis frei | Engineer 6 |
| 1 | B1 Wurf + B2 Ziel-Arten (`first`, `track_nearest`, `friendly`) + B4 Zeit-Effekte | Alchemist 18 (Perishing-Potions-Varianten 020/021/120/220 erst mit Teil 2), Mortar 29 (Signal-Flare-Varianten erst mit Teil 2), Engineer 12 (Pin, Double Gun) |
| 2 | B3 Bodenobjekte + `track_spread` + B6 Modifier/Cash, dazu Spike S2 (Bonus, Minen) und S3 (Directed) | Spike 49, Alchemist 34 (ohne Rubber to Gold), Engineer 15 (Schaum), Mortar alle 29 |
| – | **Lauf t21** (R60 → CHIMPS) mit Alchemist, Spike, Mortar, Engineer | |
| 3 | B5 Sub-Türme | Engineer 39 (Sentries; 44 erst mit der Falle E4, später) |
| 4 | Dartling (`first_global`, D1–D3), Mortar M2, Alchemist A3/A4, Spike S4 (Super Mines) | Dartling 44, Mortar 49 |
| 5 | B7 Flieger | Ace 24–42, Heli 14–29 |

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
  Turmtyp, Position und Tiers (Mortar-Ziel = nächster Streckenpunkt). Brew/Dip wirken nur während der Runde; die
  Policy sieht, wo ein Alchemist mit welchen Tiers steht. Falls Alchemisten selten gebaut werden: optionaler
  Kanal "in Reichweite eines Brew-Alchemisten" (wie die Buff-Kanäle) [Entscheidung nach dem Lauf].
- Zufallssperren bis 25 %: bei 21 Türmen 5 (heute 3) – oder bei 3 bleiben (Frage 6). Fest gesperrt: Farm (CHIMPS)
  und Türme ohne Mechanik; sie zählen nicht mit.
- Leistung: Die Place-Maske wächst mit den Turmtypen (t12 → t14: +11 % je Schritt). Erwartung für t21 grob +30 %
  Schrittzeit gegenüber t14, dazu Kosten im Tick-Loop für Bodenobjekte (Spike Factory) – messen nach Teil 2.
- `--reward-tower` bestraft auch die neuen Türme (wie die Village).

## 8. Tests und Plausibilität

Rust (btd-core), je Baustein:
- Wurf: landet nach `time` bzw. Abstand/Tempo, trifft unterwegs nichts, Explosion am Zielpunkt, nicht am Bloon
  zum Ankunftszeitpunkt.
- Ziel-Arten: `track_nearest` = nächster Streckenpunkt (Gleichstand → größere Pfad-Distanz); `track_spread` deterministisch
  und gleichmäßig über die Strecke in Reichweite; `friendly` wählt nach `spent`, dann ID; `first_global` ignoriert
  die Reichweite, Drehrate begrenzt; Spike-Intervalle folgen der gebufften Reichweite.
- Bodenobjekte: Pierce-Verbrauch mit Treffer-Reset und Kollisions-Intervall, halten die Runde nicht offen, sind
  nach der Runde weg; Distanz-Fenster gibt dasselbe Ergebnis wie die volle 2D-Prüfung (Test mit Selbstkreuzung).
- Zeit-Effekte: Ablauf nach Zeit und nach Schüssen, Sperrzeit, stärkerer ersetzt schwächeren, zwei Alchemisten
  stapeln nicht, Brew + Village wirken zusammen, Laufzeit-Zustand (Salve, Crit) bleibt erhalten, Reset zu
  Rundenbeginn.
- Sub-Türme: Erscheinen alle n s an deterministischen Positionen, Ablauf, Auren an der Position, nicht im Hash.
- Modifier: Signal Flare macht Camo sichtbar, Kinder erben; Lead to Gold gibt Cash nur für Lead-Pops.
- Flieger: Ace fliegt die Bahn mit Tempo, Heli-Pursuit erreicht den vordersten Bloon.
- Parität: sim_v0 (f64) und t12/t14-Replays bitgleich nach jedem Teil; `state_hash` zwischen Runden unabhängig von
  Zeit-Effekten und Bodenobjekten (Rundlauf über `StartState`).

btd-data: Validierung der neuen Felder (Wertebereiche, `targeting` nur mit passender Lieferart, Platzhalter nur für
gesperrte Türme), unbekannte Felder werden abgelehnt.

Konverter: Abdeckung je Turm wie in §1 (Obergrenzen), STRICT für alle neuen Modelle, Feld-Prüfungen (z.B.
`RemoveBloonModifiersModel` nur mit bekannten Schaltern), `MECHANIC_LOCKED` je Teil.

Env: t21-Layout gleich groß, egal welche Türme gesperrt sind; Maske sperrt Türme ohne Mechanik; Python
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

## 10. Offene Fragen an dich

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
