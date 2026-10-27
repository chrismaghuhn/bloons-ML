# Design: mehr Türme, Schritt 2 – Buffs & Einkommen (freigegeben 2026-10-27 mit Anpassungen, §8)

Ziel: Monkey Village und Banana Farm als Einstieg in M4 ("Buffs & Support", `docs/engine-design.md` §10),
also von 12 auf 14 Türme. Der Alchemist braucht andere Mechaniken als Village und Farm (siehe §1.3).
Er wird deshalb als eigener Schritt vorgeschlagen. Abilities (Call to Arms, IMF Loan, …) bleiben gesperrt.

Wichtigster Befund vorab: **In CHIMPS ist die Banana Farm gesperrt** (§3). Für das Endziel zählt also
vor allem die Village (Priorität 1). Die Farm ist nur für die Zwischenziele Medium/Hard relevant und kommt
nachrangig. Die Village ist stark (Camo, Tempo, Reichweite, später MIB). Mit ihr ist CHIMPS deshalb der
naheliegende nächste Benchmark (§4).

Zahlen aus dem Dump stehen hier nur als Regeln oder Platzhalter. Die konkreten Werte holt der Konverter.

## 1. Was die Türme im Dump nutzen (57.4)

Ausgewertet mit `check_types` des Konverters über alle 64 Varianten je Turm. Heute ist keine Variante
konvertierbar, weil alle drei Türme Modelle nutzen, die der Konverter nicht kennt.

### 1.1 Monkey Village (Support, 64 Varianten)

| Modell | Varianten | Wirkung (aus dem Dump) | Vorschlag |
|---|---|---|---|
| AttackModel ohne Waffen | 54 (alle außer 5xx/xx5) | nur Platzhalter (Reichweitenkreis) | ignorieren, wenn `weapons` leer |
| RangeSupportModel | 64 | Reichweite +x % (Basis); zusätzlich +x % bzw. +y für Primary (3xx/4xx) | Aura `range` |
| RateSupportModel | 26 (2xx+) | Cooldown × Faktor < 1 (Jungle Drums) | Aura `rate` |
| VisibilitySupportModel | 26 (x2x+) | Türme in Reichweite sehen Camo (Radar Scanner) | Aura `camo` |
| DiscountZoneModel | 37 (xx1+) | Rabatt, zwei Stapel mit Obergrenze (§1.1 Stapelregel) | Rabatt-Zone (§2.2) |
| PierceSupportModel, ProjectileSpeedSupportModel, FilterInSetModel | 15 (3xx+) | +Pierce / +Projektiltempo nur für Primary (`towerSet` 1, dazu gehört auch Desperado) | Aura mit Turmgruppen-Filter |
| DamageTypeSupportModel | 15 (x3x+) | MIB: Türme in Reichweite treffen alle Bloon-Arten | Aura `damage_all` |
| SupportRemoveFilterOutTagModel | 15 (x3x+) | schaltet nur `FilterOutTagModel`-Einträge ab, deren `disableWhenSupportMutatorIDs` den MIB-Mutator nennt. Bei den 12 Türmen aus t12 nennt kein Filter den MIB-Mutator (Ice hat in 19 Varianten einen anderen, wirkungslosen Eintrag) | als ausgewertet annehmen; Filter, die den MIB-Mutator nennen, per STRICT ablehnen |
| AddBehaviorToBloonInZoneModel (GrowBlock) | 37 (x1x+) | Regrow-Bloons wachsen nicht nach | ignorieren (Regrow wird nicht simuliert, wie GrowBlockModel heute) |
| CashIncreaseModel, AddBehaviorToTowerSupportModel | 15 (xx3+) | Monkey Town: mehr Cash je Pop der Türme in Reichweite | Phase 2 (Pops je Turm nötig), sonst gesperrt |
| FreeUpgradeSupportModel, AbilityCooldownScaleSupportModel | 10 (4xx+) | Gratis-Upgrades für Primary, kürzere Ability-Cooldowns | gesperrt |
| AbilityModel, CallToArmsModel | 10 (x4x+) | Ability | gesperrt (Abilities später) |
| MonkeyCity*/Monkeyopolis* | 10 (xx4+) | Verknüpfung mit der Farm, Gratis-Dart | gesperrt |
| Angriff in 5xx (RetargetOnContact usw.) | 5 | eigener Angriff | gesperrt |

Abdeckung mit Auren und Rabatt: **29/64**, mit Monkey Town 34/64.

**Stapelregel.** Vorgabe des Users: Buffs mehrerer Villages stapeln nicht, nur die Rabatte. Der Dump passt
dazu:
- Die Buff-Modelle der abgedeckten Varianten (Range/Rate/Visibility/Pierce/ProjectileSpeed/
  DamageType-Support) haben `isUnique: true` und eine feste `mutatorId`, z.B. "Range:Support",
  "Village:Rate", "Village:Visibility" oder "Village:PrimaryTraining:Range". Zwei Villages setzen also
  denselben Mutator auf den Turm, und er wirkt einmal. Die Sim nimmt je `mutatorId` den stärksten Wert.
  Ausnahmen ohne `isUnique`: `SupportRemoveFilterOutTagModel` (wirkt ohnehin nur als Schalter) und das
  gesperrte `FreeUpgradeSupportModel`.
- Die Rabatte (`DiscountZoneModel`) haben kein `isUnique`, sondern `stackName` mit `stackLimit`/
  `maxStackSize`. Der große Rabatt ("Major", Monkey Business) zählt höchstens einmal. Der kleine Rabatt
  ("Minor", Monkey Commerce) stapelt bis zu einer Obergrenze von mehr als 1. Rabatte stapeln also, aber
  begrenzt. Die Abweichung von der Vorgabe ist Frage 7.
- Innerhalb einer Village wirken verschiedene Mutatoren zusammen. Beispiel: Basis-Reichweite und
  Primary-Training-Reichweite haben verschiedene IDs. Das ist kein Stapeln über Villages. Wie das Spiel
  sie verrechnet, ist ein Kalibrierpunkt.

Weitere Felder: `appliesToOwningTower`, `isGlobal` und `isCustomRadius` sind bei allen Village-Supports aus.
Der Radius ist also die Reichweite der Village. `Range:Support` hat keinen Turmfilter, eine Village bufft
also auch eine andere Village (siehe §2.1 Radius).

### 1.2 Banana Farm (Support, 64 Varianten)

Die Farm "schießt" Bananen. Sie hat eine Waffe mit `EmissionsPerRoundFilterModel` (Bananen je Runde) und ein
Projektil mit `CashModel` (Wert je Banane, `bonusMultiplier`), `PickupModel` und Lebensdauer (`AgeModel`).
Ab xx3 (Marketplace) wird automatisch eingesammelt.

| Modell | Varianten | Vorschlag |
|---|---|---|
| CashModel, EmissionsPerRoundFilterModel | 64 | **Einkommen je Runde** = Bananen × Wert × (1 + bonus), am Rundenende gutgeschrieben |
| WeaponRateMinModel, RandomPositionBasicModel | 64 | ohne Wirkung, wenn die Sim alles einsammelt (Annahme, §2.3) |
| SingleEmmisionTowardsTarget, ArriveAtTarget, HeightOffset, Pickup, ScaleProjectile | 34–49 | ebenso |
| CollectCashZoneModel | 5 (xx5) | ebenso |
| RectangleFootprintModel | 64 | quadratischer Footprint (§2.3) |
| `sellbackModifierAdd` (Feld am Turm) | 26 (ab xx2) | höherer Verkaufswert, nur bei der Farm > 0 → `VariantV1.sell_bonus` |
| PerRoundCashBonusTowerModel, BonusLivesPerRoundModel | 10/5 (xx4+, xx5) | fester Bonus je Runde (Cash, Leben) |
| BananaCentralBuffModel | 5 (5xx) | globaler Einkommens-Faktor; der Dump nennt keinen Filter, welche Farmen er trifft [KAL, Spielwissen] |
| CentralMarketBuffModel | 10 (xx4+) | laut Spielwissen für Merchantmen (Buccaneer, nicht im Set) → ignorieren [KAL] |
| BankModel, SendToBankModel | 15 (x3x+) | Bank mit Zinsen und Kapazität: braucht Zustand und Abheben → Phase 2 |
| ImfLoanModel, ImfLoanCollectionModel, AbilityModel | 10 (x4x+) | Ability → gesperrt |

Abdeckung ohne Bank: **49/64**, mit Bank 54/64.

Unklar im Dump: das Feld `salvage` am CashModel (wächst über xx1–xx3). Vermutlich ist es der Anteil
abgelaufener Bananen, der trotzdem gutgeschrieben wird [KAL]. Wenn die Sim alles einsammelt, wirkt es
nicht. Für die Bridge (M5) ist es wichtig.

### 1.3 Alchemist (64 Varianten) – Vorschlag: eigener Schritt

Schon die Basis braucht **geworfene Tränke**: `ArriveAtTargetModel`, das Projektil fliegt zur Zielposition
und platzt dort. Dazu kommt `AcidPoolModel` als kurzer Splash beim Aufschlag. Echte Säurepfützen mit
Lebensdauer gibt es erst ab xx2 (`AcidPoolEmissionModel`).

Die Buffs werden per Projektil auf einen anderen Turm geworfen (`TargetFriendlyModel`). Das sind Acidic
Mixture Dip (26 Varianten, mit Ladungen) und Berserker Brew (15 Varianten, mit Dauer). Das ist die zweite
Buff-Art aus `engine-design.md` (`timed_effects`), nicht die statische Aura der Village.

Dazu kommen:
- Lead to Gold ab xx3 (`GiveCashOnPop`)
- Transforming Tonic 050 (`MorphTowerModel`)
- Bloon Master Alchemist xx5 (`MorphBloonModel`)

Mit Village-Mechaniken allein ist keine Alchemist-Variante abgedeckt.

## 2. Neue Engine-Mechaniken

### 2.1 Statische Auren (Village)

Datenformat (`btd-data/v1.rs`, alles `deny_unknown_fields`). Die Werte im Beispiel sind Platzhalter:

```json
"supports": [
  {"kind": "range", "add_frac": "<x>", "add": 0.0, "group": "Range:Support"},
  {"kind": "rate", "factor": "<f<1>", "group": "Village:Rate"},
  {"kind": "camo", "group": "Village:Visibility"},
  {"kind": "pierce", "add": "<n>", "sets": ["primary"], "group": "Village:PrimaryTraining:Pierce"},
  {"kind": "projectile_speed", "add_frac": "<x>", "sets": ["primary"], "group": "..."},
  {"kind": "damage_all", "group": "Village:DamageType"}
]
```

Die Bedeutung ist je `kind` festgelegt:
- `add_frac` ist ein Zuschlag. Effektiv gilt Basis × (1 + Σ add_frac) + Σ add, und je Gruppe zählt der größte.
- `factor` ist ein Multiplikator. Er wird je Gruppe als kleinster genommen (beim Cooldown ist das der
  stärkste) und über Gruppen multipliziert.

Format:
- `VariantV1.supports` ist neu, Standard leer.
- `TowerDefV1.set` ist neu (`primary|military|magic|support`, aus `towerSet` des Dumps). Standard ist
  `primary`, damit ältere JSONs (z.B. sim_v0) weiter laden.

Validierung (neu, heute prüft `GameV1::validate` leere `attacks` gar nicht):
- Eine Variante ohne Angriff braucht `supports`, `discount` oder `income`.
- `add_frac` > −1, `factor` in (0, 1], `group` nicht leer, `sets` nur bekannte Namen.

Engine (`btd-core`):
- `Tower` behält die Werte der Variante als **Basis** und berechnet daraus die **effektiven** Angriffe
  (`AttackRt`).
- `Game::refresh_buffs()` läuft nach jeder Aktion (Bauen, Upgraden, Verkaufen), wann immer sie passiert,
  also auch mitten in der Runde: an genau dem Aktionspunkt zwischen zwei Ticks, sobald die Engine Runden
  in Ticks mit Aktionen dazwischen spielt (`docs/design-recorder.md`, Schritt 3). Im Tick-Loop läuft es
  nie. Außerdem läuft es nach `reset()`, nach `StartState::build()` und nach dem Laden eines Zustands.
- `refresh_buffs` baut die Angriffe **nicht** über `set_variant` neu. Es schreibt nur die
  buff-abhängigen Felder (`range2`, `cooldown`, `camo`, `cannot_damage`, Pierce, Flächenradius,
  `ProjectileDef`). So bleibt der Laufzeit-Zustand vollständig erhalten. Das gilt auch für Felder, die
  `set_variant` heute zurücksetzt: `burst_left`/`burst_cd` (laufende Salve) und `crit_next`.
  Projektile, die schon fliegen, behalten ihre alte `ProjectileDef`. [KAL über den Recorder: setzt ein
  Upgrade im Spiel den Cooldown zurück?]
- Es baut die effektiven Werte neu: für jeden Turm alle Auren anderer Türme, deren Mittelpunkt-Abstand
  ≤ Radius ist [KAL: zählt der Footprint des Ziels mit?].
- **Radius = unbuffte Reichweite der Variante.** Eine Village, die eine andere bufft, vergrößert deren
  Aura nicht. Damit reicht ein Durchgang, ohne Fixpunkt und ohne Abhängigkeit von der Reihenfolge
  [KAL: im Spiel evtl. anders].
- Stapeln wie in §1.1: je Gruppe (`mutatorId`) wirkt nur die stärkste Aura, egal wie viele Villages in
  Reichweite sind. Bei Gleichstand gilt die des ältesten Turms, damit es deterministisch ist.
  [KAL: Modifier-Reihenfolge, engine-design §9]

Wirkung je `kind`:
- `range` → `range2`. Flächenangriffe, deren Radius im Spiel an die Reichweite gekoppelt ist
  (`LinkProjectileRadiusToTowerRangeModel`: Ice, Tack 4xx Ring of Fire), brauchen dafür ein neues
  Format-Feld. Heute schreibt der Konverter `area.radius` als festen Wert. Neu kommt `radius_offset`
  dazu, und gebufft gilt Radius = effektive Reichweite + Offset. Unbufft bleibt der gespeicherte Wert
  unverändert (Parität).
- `rate` → `cooldown` [KAL: auch der Salven-Abstand?].
- `camo` → `camo` am Angriff, an den Projektilen und an den Kontakten.
- `pierce` → Pierce des Haupt-Projektils. Nicht bei Sofort-Treffern (Desperado: `pierce 1` heißt dort
  "nur das Ziel") und nicht bei Flächenangriffen (Tack 4xx) [KAL: wirkt es im Spiel dort, und an
  Kontakt-Projektilen?].
- `projectile_speed` → eigene Kopie der `ProjectileDef`.
- `damage_all` → `cannot_damage` leer, am Angriff **und an allen Kontakten** (Explosion, Splitter). Sonst
  trifft eine Bomb-Explosion trotz MIB kein Black. `exclude_tags` bleiben (siehe §1.1). Ob MIB auch
  `skip_props` aufhebt (z.B. den Lead-Filter von Ice), ist ein Kalibrierpunkt.

Zonen (Glue-/Slow-Zonen) bleiben ungebufft [KAL: `bindRadiusToTowerRange`].

### 2.2 Rabatt-Zone (Village xx1/xx2)

Format an der Variante: `"discount": [{"fraction", "tier_min", "tier_cap", "stack", "stack_limit", "affect_self"}]`.
Alle Felder kommen aus dem Dump:
- `tierMin` ist 0, also gilt der Rabatt auch beim Platzieren des Basisturms.
- `tierCap` legt das höchste rabattierte Tier fest.
- `affectSelf` ist aus, die Village rabattiert ihre eigenen Upgrades nicht.
- `towerBaseIdList` ist leer, der Rabatt gilt also für alle Türme.

Der Preis eines Kaufs auf Tier in [`tier_min`, `tier_cap`] ist Medium-Preis × (1 − Σ Rabatt) ×
Schwierigkeit, auf 5 $ gerundet. Je Stapel zählen höchstens `stack_limit` Zonen.
[KAL: Rundungsreihenfolge Rabatt/Hard; Platzieren ist durch `tierMin` nur indirekt belegt]

`spent` speichert den bezahlten Preis. Der Verkaufswert stimmt damit automatisch.

Folgen:
- Der Preis hängt vom Ort ab. Statt `price(cost)` gibt es `Game::place_price(kind, x, y)` und
  `upgrade_price(tower, path)`. Beide nutzen `discount_at(x, y)`.
- `check`, `place_price` und `upgrade_price` rechnen immer genau mit `discount_at`, an der echten Position.
- **Rabatt-Raster als Cache, nur für die Place-Maske und die Observation.** `refresh_buffs` berechnet je
  Rasterzelle den Platzier-Rabatt (Tier 0) mit `discount_at` am Zellmittelpunkt. Dort platziert die Env.
  - Die Place-Maske (14 Türme × 720 Zellen) liest nur dieses Raster und prüft das Cash **je Zelle**. Heute
    überspringt sie die ganze Turmart, wenn der Listenpreis zu hoch ist. Weil Raster und `check` dieselbe
    Funktion am selben Punkt nutzen, stimmen sie für Env-Platzierungen überein.
  - Die Upgrade-Maske nutzt wie heute `can_apply` und damit den genauen Preis an der Turmposition. Das
    gilt auch für Türme, die nicht am Zellmittelpunkt stehen (Replays, Pools, Störung "Verschieben").
  - Dasselbe Raster ist der Rabatt-Kanal der Observation (Platzier-Rabatt am Zellmittelpunkt; Upgrades
    über Tier `tier_cap` sind nicht rabattiert, das sieht die Policy an den Tiers).
- `spent` hängt jetzt von der Reihenfolge der Käufe ab. `StartState` speichert `spent` bisher nicht, und
  `build()` baut Turm für Turm komplett auf. Ein gespeicherter Zustand mit Rabatt käme so mit anderem
  `spent` zurück (Verkaufswert, `state_hash`). Deshalb kommt `spent` in `TowerState` (optional). `build()`
  setzt es nach dem Aufbau. Fehlt das Feld (alte Pools), gilt wie bisher der beim Aufbau bezahlte Preis.
- Störung "Upgrade zurück" (`start::perturb`): Ein einzelnes `spent` reicht nicht, um genau ein Upgrade zu
  erstatten. `perturb` arbeitet außerdem auf dem `StartState` vor dem Aufbau, ein `Game` mit diesem
  Aufbau gibt es dort nicht. Festlegung:
  - `perturb` merkt sich nur, welches Upgrade zurückgenommen wird (wie heute `sell` beim Verkaufen).
  - Erst nach `build()` stuft `Game::downgrade(tower, path)` (neue Engine-Operation, nur für Startzustände)
    am gebauten Spiel zurück. Erstattet wird der dann gültige `upgrade_price` dieses Upgrades an der
    Turmposition. `spent` sinkt um denselben Betrag (nicht unter 0), danach `refresh_buffs`.
  - Damit ist auch eine zurückgestufte Village sauber abgedeckt (der Rabatt der anderen Türme ändert sich
    erst danach). Die Reihenfolge ist fest: erst alle Zustandsänderungen am `StartState` (Cash,
    Verschieben), dann Aufbau, dann Zurückstufen und Verkaufen.
  - Das ist eine Näherung, falls der Turm im Original zu einem anderen Rabatt gekauft wurde.
    "Verschieben" lässt `spent` unverändert.

### 2.3 Einkommen (Farm)

Erkennung im Konverter: eine Waffe mit `EmissionsPerRoundFilterModel`, deren Projektil ein `CashModel` hat,
ist Einkommen und kein Angriff. Diese Modelle sind STRICT und nur dort erlaubt.

Format:
- `VariantV1.income: {"per_round": f64, "lives": i32}`. `per_round` ist Bananen × Wert × (1 + bonus) plus
  fester Rundenbonus.
- `VariantV1.sell_bonus` (Standard 0) kommt auf `sell_ratio` drauf [KAL: additiv?].
- Banana Central: `income.global_mul` mit Gruppe. Welche Farmen er trifft, ist Spielwissen, nicht im Dump
  [KAL].

Die Gutschrift kommt nach jeder geschafften Runde zusammen mit `end_bonus`, in der Reihenfolge der Turm-IDs.

Annahmen:
- Bananen werden eingesammelt [KAL]. Ohne Zufall sind es 100 %, auch immer in der Evaluation. Mit
  `randomness` würfelt die Env je Episode eine Einsammelquote, z.B. 85–100 %. So verlässt sich die Policy
  nicht auf perfekte Klicks der Bridge.
  - Betroffen sind nur Farmen, deren Bananen geklickt werden müssen: unterer Pfad < 3 (ab Marketplace
    sammelt das Spiel selbst) und mittlerer Pfad < 3 (die Bank bekommt kein Bananen-Projektil).
    xx1/xx2 vergrößern im Spiel den Einsammelradius bzw. erstatten abgelaufene Bananen anteilig; das
    bildet die Sim nicht ab [KAL].
  - Die Quote ist ein Feld in `Game` (Standard 1,0). Sie geht in `state_hash`, ins Replay-Format und
    optional in `StartState`, damit sie deterministisch reproduzierbar ist.
  - Teil-Einkommen wird wie anderes Einkommen über `Money::from_f64` auf 1/1000 $ gerundet.
- Bananen-Cash fällt nicht unter die Einkommens-Stufen der späten Runden (`incomeSets`) [KAL].
- Eine neu gebaute Farm liefert schon in der nächsten Runde voll.

Footprint:
- Die Farm hat ein Quadrat. Ein Kreis mit halber Seitenlänge (Innenkreis) würde die Ecken über Track oder
  andere Türme ragen lassen. Vorschlag: Außenkreis (halbe Diagonale) für Platzierung und Kollision.
  Etwas strenger als im Spiel, aber nie ungültig. Alternative wäre ein echter Quadrat-Check [KAL].
- Das `buildable`-Raster der Observation wird heute mit dem größten Footprint aller Türme gerechnet. Mit der
  Farm würde es deutlich kleiner. Vorschlag: das Raster weiter mit dem größten Footprint der übrigen Türme
  rechnen. Die Maske je Turm (`map_ok`) prüft ohnehin den eigenen Footprint.

### 2.4 Turm-Sperren aus den Regeln

`RulesV1.locked_towers: ["farm"]` ist neu (Standard leer). Der Konverter übernimmt es aus
`LockTowerModModel` der Partie-Mods des Presets, statt Türme von Hand auszuschließen.

Gesperrte Türme bleiben in den Daten, damit Turmliste und Layout für alle Presets gleich bleiben. Für die Env:
- Die Maske verbietet sie immer.
- Die Sperr-Skalare der Observation zeigen sie als gesperrt. Heute gibt es diese Skalare nur, wenn
  Zufallssperren an sind (`Layout::new(.., max_banned_towers > 0)`). Damit das Layout nicht von der
  Konfiguration abhängt, sind sie ab t14 immer im Layout (bei 0 Zufallssperren zeigen sie nur die festen
  Sperren).
- `draw_bans` würfelt sie nicht. Sie zählen auch nicht zur Obergrenze der Zufallssperren.
- Startzustände oder Replays mit einem gesperrten Turm (z.B. aus einer echten Medium-Partie mit Farm)
  werden beim Laden mit einer klaren Meldung abgelehnt.
- Der Konverter lehnt unbekannte Sperr-Mods in den Mods des Presets ab (STRICT), z.B.
  `LockTowerSetModModel` aus den `*Only`-Modi.

**Farm von Anfang an im Layout.** Jede Änderung der Turmliste macht bestehende Modelle unbrauchbar. Die Farm
steht deshalb ab dem ersten Village-Schritt als 14. Turm in Daten und Layout, ist aber in allen Presets
gesperrt. Der Konverter setzt sie zusätzlich zu den Mod-Sperren auf `locked_towers`, solange
`--towers t14` die Farm-Mechanik noch nicht freigibt. Später wird nur die Sperre für Medium/Hard
aufgehoben. Kanäle und Aktionsraum ändern sich dabei nicht. Damit die Daten gültig sind, schreibt der
Konverter die Farm-Varianten schon im Format aus §2.3. Gesperrt ist es egal, ob die Engine das Einkommen
schon gutschreibt.

**Verkaufen.** CHIMPS verbietet auch das Verkaufen. Das ist schon abgedeckt: `RulesV1.selling` steht im
Preset `chimps` auf `false`. `Game::check` lehnt `Sell` dann ab, die Env-Maske schaltet die Sell-Aktionen
ab, und die Startzustands-Störung "Verkaufen" wird nicht gezogen. Neu ist nur: Der Konverter leitet
`selling` aus `DisableSellTowerModModel` der Partie-Mods ab (gegengeprüft mit dem Partie-Feld
`towerSellEnabled`), wie die Turm-Sperren. Heute steht es von Hand im Preset; Werte und Verhalten bleiben
gleich. `SellMultiplierModModel` (Verkaufsquote, in `Clicks` auf 0) wird dafür nicht genutzt. `sell_ratio`
bleibt wie heute fest, sonst würde es sich in CHIMPS ändern.

### 2.5 Determinismus

- Ohne Village und Farm in den Daten bleibt alles beim Alten: effektiv = Basis, Area-Radien wie
  gespeichert, Preise über dieselbe Rechnung. sim_v0-Parität und bestehende Replays bleiben bitgleich
  (Test, §5).
- Buffs werden nur bei Aktionen neu berechnet, nicht pro Tick, und in fester Reihenfolge (Turm-ID).
  Für die Buffs braucht `state_hash` nichts Neues, weil sie aus den Türmen folgen. Neuer Zustand kommt
  nur mit der Farm: die Einsammelquote (§2.3), später die Bank (Phase 2). Das Rabatt-Raster ist ein Cache
  und gehört nicht in den Hash.
- Rechnung in f64, Umwandlung in `Real` wie in `set_variant`. Die Radius-Grenze (≤) kann sich zwischen
  f32- und f64-Build in Grenzfällen unterscheiden, wie heute schon bei `collides`.

## 3. Spielregeln je Modus (Dump `matches/` und `gamedata/mods.json`)

- **CHIMPS** (`Standard_Clicks_Hard_Tutorial`, Mods `[Hard, Clicks]`): Der Mod `Clicks` enthält
  `LockTowerModModel` mit `towerToLock: BananaFarm`. Die Farm ist dort also gar nicht baubar. Auch der
  Deflation-Mod sperrt sie.
- Außerdem in `Clicks`: `ChimpsModModel` (ohne Felder, Wirkung im Dump nicht sichtbar), kein Verkaufen,
  kein Continue, kein Monkey Knowledge. Start- und Endrunde sowie die Leben wie im bisherigen Preset.
- Der Rundenbonus (`BonusCashPerRoundModel`) steht auch in der CHIMPS-Partie. Das deckt sich mit dem
  bisherigen Preset.
- Village-Einkommen (Monkey Town/City) sperrt kein Mod ausdrücklich. Ob `ChimpsModModel` es abschaltet,
  lässt sich aus dem Dump nicht belegen. Monkey Town kommt deshalb erst in Phase 2 und wird dann im Spiel
  geprüft.
- Medium Standard (Partie im Dump): keine Turm-Sperre. Für Hard Standard gibt es keine Partie in `matches/`.
  Laut `mods.json` sperrt der Mod `Hard` keinen Turm.

## 4. RL-Env und Observation

- 14 Türme, Reihenfolge: die bisherigen 12, dann `village`, `farm`. Ausgabe z.B. mit `--towers t14`.
  - Kanäle: 2 + 14 + 3 = 19, mit den Buff-Kanälen (unten) 24.
  - Aktionen: 1 + (14 + 3 + 1) × 720 = 12 961.
  - Bestehende Modelle passen nicht, es braucht neue Läufe (wie bei t12).
- Zufallssperren bis 25 %: bei 14 Türmen weiterhin 3. Die Farm ist vorerst in allen Presets fest
  gesperrt, in CHIMPS auch dauerhaft (§2.4). Sie zählt nicht zu den 3.
- Was die Policy über Buffs wissen muss: Die Auren folgen aus Position und Tiers der Village. Die
  Conv-Policy (Radius ≈ 4–6 Zellen) müsste sie aber erst mühsam erschließen. Außerdem bringt eine Village
  selbst keine Pops; ihr Wert kommt verzögert über andere Türme, daran lernt PPO langsamer. Deshalb
  **5 zusätzliche Kanäle**, jeweils der wirksame Wert je Zelle: Reichweiten-Buff, Tempo-Buff, Camo, MIB
  (trifft alle Bloon-Arten: für CHIMPS entscheidend bei Lead/Purple/Black) und Rabatt (das Raster aus
  §2.2). Das ergibt 2 + 14 + 3 + 5 = **24 Kanäle**. Pierce und Projektiltempo bekommen keinen Kanal.
- Beobachten: Im Festhäng-Histogramm bzw. in der Turmstatistik der Auswertung gezielt prüfen, ob Villages
  überhaupt gebaut werden.
- Skalar: Farm-Einkommen der nächsten Runde, log-normiert, weil es ab 4xx schnell groß wird. Er steht
  von Anfang an im Layout (solange die Farm gesperrt ist, immer 0).
- Benchmark: Nach dem R60-Lauf mit Village kommt ein CHIMPS-Lauf. Preset `chimps`, die Farm per
  `locked_towers` gesperrt, Curriculum aus engine-design §5 (erst Leben-Puffer, dann 1 Leben). Erwartung:
  Die Village macht vor allem die Camo-Runden und die späten MOAB-Runden leichter.
- `--reward-tower` bestraft auch Village und Farm. Das ist ok, beim Vergleich der Läufe aber beachten.
- Startzustände: `TowerState` verweist per Name, alte Pools laden also auch mit t14 (außer sie enthalten
  einen gesperrten Turm, §2.4). Zufalls-Builds nutzen
  die neuen Preisfunktionen und respektieren `locked_towers`.

## 5. Tests und Plausibilität

Rust (btd-core):
- Aura wirkt nur innerhalb des Radius.
- Gleiche Gruppe stapelt nicht über zwei Villages, verschiedene Gruppen einer Village schon.
- Verkauf der Village nimmt den Buff weg. Eine Village, die nach dem Turm gebaut wird, gibt ihn.
- Village-auf-Village: Der Aura-Radius bleibt unbufft.
- Camo-Aura lässt Dart Camo treffen.
- MIB lässt Bomb (Explosion) Black treffen, Glue bekommt aber keine MOAB-Treffer.
- Ring of Fire und Ice wachsen mit dem Reichweiten-Buff.
- Pierce-Buff ändert Desperado nicht.
- Rabatt: Stapel-Limits, Tier-Grenzen, Rabatt beim Platzieren, Verkaufswert aus dem bezahlten Preis.
- Farm-Einkommen nur nach geschaffter Runde.
- `locked_towers` blockiert `Place`.

Parität: sim_v0-Goldtrace und vorhandene Replays (t12) bleiben unverändert (bitgleiche Hashes), auch für
Tack 4xx und Ice mit dem neuen `radius_offset`.

btd-data: Validierung (Variante ohne Angriff nur mit Support/Rabatt/Einkommen, Wertebereiche,
`set`-Standard), unbekannte Felder werden abgelehnt.

Konverter:
- Abdeckung wie oben (Village 29, Farm 49).
- `locked_towers`: Farm in allen Presets, in CHIMPS zusätzlich aus `LockTowerModModel`.
- `selling` aus den Mods ist gleich dem bisherigen Preset-Wert (Medium an, CHIMPS aus).
- STRICT für die neuen Modelle (nur an der ausgewerteten Stelle erlaubt).
- Filter, deren `disableWhenSupportMutatorIDs` den MIB-Mutator nennt, werden abgelehnt (Ice bleibt
  unverändert abgedeckt).

Weitere Tests:
- Startzustände: Rundlauf mit Rabatt (`spent` bleibt gleich).
- "Upgrade zurück" mit Rabatt erstattet den Preis am Ort, auch wenn eine Village zurückgestuft wird.
- Ein Refresh mitten in der Runde ohne Buff-Änderung ändert nichts am Zustand (Salve, Crit, Cooldown).
- Das Rabatt-Raster ist gleich `discount_at` an allen Zellmittelpunkten.
- Die Einsammelquote ist deterministisch und reist im Replay mit.
- Das t14-Layout ist gleich groß, egal ob die Farm gesperrt ist und ob Zufallssperren an sind.

Plausibilität:
- `btd-tools demo`: Dart allein gegen Dart + Jungle Drums (Pops je Runde).
- Farm-Cash über 10 Runden gegen die Regel Bananen × Wert.

Abgleich mit dem Spiel über den Recorder statt per Auge: Die Mod dumpt nach jedem Bauen, Upgraden und
Verkaufen die effektiven Werte jedes Turms aus dem Spiel. Das sind Reichweite, Rate, Camo,
Schadensarten und der Preis des nächsten Upgrades, dazu der Platzierpreis an der Position. Ein einziger
Testaufbau beantwortet damit mehrere Fragen:
- Zählt der Footprint des gebufften Turms zur Reichweite?
- Bufft eine Village die Reichweite einer anderen?
- Wie verrechnen sich Basis- und Primary-Training-Reichweite?
- Gilt der Rabatt beim Platzieren, und wie wird mit Hard gerundet?

Der Sim-Test vergleicht dieselben Werte (`btd-tools`, gleiche Aktionsfolge). Das ist eine Erweiterung der
Mod (eigener Teilschritt, Ordner `mod/`). Nur "Monkey Town in CHIMPS" braucht eine echte Partie (Phase 2).

## 6. Reihenfolge

1. Konverter und btd-data:
   - Support-Türme (Waffe leer bzw. Einkommens-Waffe)
   - `supports`/`discount`/`income`/`sell_bonus`/`set`/`radius_offset`/`locked_towers`
   - `selling` und `locked_towers` aus den Partie-Mods; Farm in allen Presets gesperrt
   - Validierung
   - Noch ohne Wirkung in der Engine, die Parität ist damit sicher.
2. Engine: Auren und Rabatt (Village 29/64), Tests, Parität.
3. Env: 14 Türme (Farm im Layout, überall gesperrt), 5 Buff-Kanäle, Einkommens-Skalar, Rabatt-Raster
   und Preis-Maske, feste Sperren, `spent` in Startzuständen. Danach Lauf R60 und CHIMPS.
4. Recorder-Erweiterung: effektive Werte nach jeder Aktion dumpen, Abgleich mit der Sim (Kalibrierpunkte §5).
5. Farm: Einkommen, Footprint, Verkaufswert, Einsammelquote (Farm 49/64). Erst wenn CHIMPS mit Village
   läuft, dann Sperre für Medium/Hard aufheben.
6. Später/optional:
   - Monkey Town (Pops je Turm)
   - Bank (Zustand und automatisches Abheben)
   - Alchemist als Schritt 3 (geworfene Projektile, Säurepfützen, `timed_effects`)

## 7. Offene Fragen (beantwortet, siehe §8)

1. Alchemist als eigenen Schritt 3 (Vorschlag) statt jetzt?
2. Farm nachrangig und nur für Medium/Hard (Vorschlag), oder ganz weglassen, bis CHIMPS läuft?
3. Ist die Annahme "alle Bananen werden eingesammelt" ok? Die Bridge müsste dann klicken.
4. Monkey Town und Bank gleich mit (dann 34 bzw. 54 Varianten) oder erst gesperrt lassen?
5. Buff-Kanäle in der Observation (ursprünglich 4 vorgeschlagen; Entscheidung in §8: 5) oder nur über die Maske?
6. Magst du im Spiel die Kalibrierpunkte aus §5 prüfen?
7. Rabatte stapeln laut Dump nur begrenzt (großer Rabatt einmal, kleiner bis zu einer Obergrenze). So
   übernehmen, oder sollen Rabatte unbegrenzt stapeln?

## 8. Freigabe und Entscheidungen (2026-10-27)

Der User hat das Design mit diesen Anpassungen freigegeben (eingearbeitet in §2–§6):
- Verkaufen in CHIMPS: über das vorhandene `RulesV1.selling` abgedeckt, künftig aus den Mods abgeleitet (§2.4).
- Farm ab sofort als 14. Turm in Daten und Layout, überall gesperrt, bis die Mechanik kommt (§2.4, §6).
- Störung "Upgrade zurück": Erstattung = aktueller `upgrade_price` am Ort, nach dem Aufbau am gebauten
  Spiel; `spent` in `TowerState` (§2.2).
- `refresh_buffs` nach jeder Aktion, auch mitten in der Runde, nie im Tick-Loop (§2.1).
- Rabatt-Raster als Cache für Place-Maske und Observation (§2.2).
- Villages bringen selbst keine Pops: Buff-Kanäle und gezielte Beobachtung, ob Villages gebaut werden (§4).

Antworten auf §7:
1. Alchemist: eigener Schritt 3 (Timed Effects, geworfene Projektile, eigener Paritätsschritt).
2. Farm: jetzt in Layout und Daten, die Mechanik erst, wenn CHIMPS mit Village läuft.
3. Bananen: Annahme ok; Einsammelquote als Domain Randomization (z.B. 85–100 %), nur Medium/Hard.
4. Monkey Town und Bank: gesperrt lassen, bis der Rest kalibriert ist.
5. Buff-Kanäle: ja, mit MIB als fünftem Kanal; Pierce und Projektiltempo ohne Kanal.
6. Kalibrierung über die Recorder-Mod (effektive Werte nach jeder Aktion), nur Monkey Town in CHIMPS
   braucht eine echte Partie.
7. Rabatt-Stapel wie im Dump (begrenzt). Unbegrenztes Stapeln würde Village-Ketten belohnen, die im Spiel
   nichts bringen.
