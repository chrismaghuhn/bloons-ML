# BTD6-Sim: Engine-Design (Rust)

Ziel: schnelle, deterministische, headless BTD6-Simulation für RL (MaskablePPO),
die nah genug am echten Spiel ist, um die gelernte Policy später per
Screen-Capture-Bridge im echten BTD6 spielen zu lassen.

Nicht-Ziele: bit-genaue Kopie des Spiels, Grafik im Core, Online-/Mod-Anbindung.

Stellen, die wir gegen das echte Spiel abgleichen müssen, sind mit **[KAL]** markiert
und in §9 gesammelt.

---

## 1. Workspace-Layout

```
engine/
├── Cargo.toml              # [workspace], rust-version = "1.80" (round_ties_even ab 1.77)
├── crates/
│   ├── btd-data/           # Datenformate + Laden/Validieren (serde), keine Logik
│   ├── btd-core/           # Simulation, kein IO, kein Python
│   ├── btd-env/            # RL-Schicht: Aktionsraum, Masken, Observation, Rewards
│   ├── btd-py/             # PyO3-Bindings (maturin) -> `import btd`; enthält auch die Pytest-Paritätstests
│   └── btd-tools/          # CLI (reines Rust): bench, replay, validate-data
data/                       # Schichten-Modell, Details in §4
├── raw/                    # generiert aus dem Wiki, nie von Hand editiert
├── base/                   # normalisiert, das liest btd-data
├── patches/                # Overrides in fester Reihenfolge (wie Civs <Update>)
└── schema/                 # JSON Schema für base/ und patches/
```

**Abhängigkeitsrichtung (strikt, keine Zyklen):**

```
btd-data  <-  btd-core  <-  btd-env  <-  btd-py
                  ^             ^
                  └── btd-tools ┘
```

- `btd-core` kennt kein JSON und kein Dateisystem: es bekommt fertige,
  validierte Defs aus `btd-data` (`GameData`) per `Arc`.
- `btd-core` kennt kein Grid und keinen Aktionsindex: das ist RL-Kram und gehört
  in `btd-env`. Der Core arbeitet mit kontinuierlichen Koordinaten in Spieleinheiten.
- Nur `btd-py` hängt an PyO3. `btd-tools`, Rust-Tests und Benchmarks laufen ohne Python.

## 2. Externe Crates

| Crate | Wo | Wofür |
|---|---|---|
| `serde`, `serde_json` | data | JSON laden |
| `thiserror` | data, core | Fehlertypen |
| `rand_core` + `rand_pcg` (`Pcg32`) | core | seeded, portabler RNG, stabil über Versionen |
| `libm` | core | plattformstabile `sin/cos/atan2/powf` (System-libm unterscheidet sich je OS) |
| `xxhash-rust` (xxh3) | core | `state_hash` und RNG-Seed-Ableitung mit festem Seed |
| `glam` (`Vec2`, f32) | core | Vektormathe, nur `+ - * / sqrt` (die sind IEEE-exakt) |
| `smallvec` | core | Kinderlisten, Trefferlisten ohne Heap |
| `rayon` | env | viele Spiele parallel (ein Spiel bleibt single-threaded) |
| `pyo3`, `numpy` (rust-numpy) | py | Bindings, Observation zero-copy als ndarray |
| `clap` | tools | CLI |
| `criterion` | dev | Benchmarks |
| `proptest`, `insta` | dev | Property-Tests, Snapshot-/Golden-Tests |

Bewusst **nicht**:
- `bevy_ecs` oder ein anderes ECS. Wir haben 2 Entitätsarten (Bloons, Türme) plus
  Projektile. Einfache Structs/SoA-Vecs sind schneller, deterministischer und leichter
  zu debuggen.
- `std::collections::hash_map::DefaultHasher` für irgendetwas Persistentes (Ausgabe nicht
  über Rust-Versionen garantiert). `HashMap` wird im Core nicht iteriert, nur `Vec` mit
  stabilen IDs.
- `f32::sin/cos/…` aus `std` im Sim-Pfad. Ein Clippy-Lint (`disallowed_methods`) erzwingt `libm`.

## 3. Determinismus und "Unity-Verhalten"

- **Sim-Zustand in `f32`** wie Unity (Positionen, HP, Timer). Ausnahme: Cash (s. u.).
- **Plattformstabil:** nur IEEE-exakte Operationen (`+ - * / sqrt`) direkt. Alles
  Transzendente (Winkel für Tack/Streuung, `powf`) über `libm`. Keine FMA-Kontraktion
  (Rust macht das standardmäßig nicht, aber kein `mul_add` verwenden).
- **Fester Tick:** `TICK_RATE = 60` (in `Rules` konfigurierbar). Zeiten aus den Daten
  werden in Ticks umgerechnet, Cooldowns zählen in Ticks herunter. **[KAL]**
- **Runden wie C#:** Modul `core::unity` mit `round()` = `f32::round_ties_even`
  (Banker's Rounding wie `Mathf.Round`/`Math.Round`), `floor_to_int`, `clamp` usw.
- **Cash als Fixed-Point:** `Money(i64)` in 1/1000 $. Grund: nach Runde 50 gibt es
  Bruchteil-Cash pro Pop, tausende kleine Additionen würden in `f32` sichtbar driften.
  Cash ist nicht im Hot-Path, die Kosten sind egal. Gerundet wird nur an den Stellen,
  an denen das Spiel rundet (Kaufpreise mit Schwierigkeit/Rabatt, Sell-Wert, Anzeige). **[KAL]**
- **RNG pro Runde ableiten:**
  `Pcg32::new(xxh3(seed, round, Purpose::Spawn | Combat | Noise))` zu Rundenbeginn.
  Ein zusätzlicher Würfelwurf an einer Stelle verschiebt nichts anderes, und jede Runde ist
  aus (Seed, Zustand zu Rundenbeginn) reproduzierbar. Das ist die Basis für Replays.
- **Iterationsreihenfolge fix:** Türme nach Bau-Reihenfolge, Bloons nach Spawn-ID.
- **`state_hash()`:** xxh3 mit festem Seed über alle relevanten Felder, Floats via
  `to_bits()`. Test: gleicher Seed + gleiche Aktionen ⇒ gleicher Hash nach jeder Runde,
  auch auf Linux/Windows/macOS in CI.

## 4. `btd-data`

### Datenformat und Schichten

Format ist **JSON** überall dort, wo Daten generiert werden (Wiki → Python → JSON → `serde`).
Nicht XML: `serde` + JSON bildet die Enums des Datenmodells direkt ab, die Wiki-API und die
Python-Skripte liefern JSON, und XML wäre bei 26 Türmen × 15 Upgrades sehr aufgebläht.
Was wir von Civ übernehmen, ist das Prinzip: Daten getrennt vom Code, ein Eintrag pro
Einheit, Overrides als eigene Schicht, Schema-Validierung.

```
data/
├── raw/                          # Schicht 0: Wiki-Rohdaten, generiert, nie von Hand editiert
│   ├── towers/*.json             #   Rohformat: data/raw/towers/SCHEMA.md
│   ├── rounds.json
│   └── bloons.json
├── base/                         # Schicht 1: normalisiert, strikt, das liest btd-data
│   ├── towers/<name>.json        #   ein Turm pro Datei, Angriffe mit `tag`, Modifier mit Selector
│   ├── bloons.json
│   ├── rounds.json               #   Gruppen mit kind, flags, count, start, end
│   └── maps/<name>.json          #   Pfad-Polylinie, Sperrflächen, Wasser
├── patches/                      # Schicht 2: Overrides, in Dateinamen-Reihenfolge angewendet
│   ├── 010_wiki_fixes.json       #   bekannte Wiki-Fehler/Widersprüche (z.B. Mortar-Camo 0-0-1)
│   ├── 020_missing_values.json   #   Werte, die im Wiki fehlen (null in raw/)
│   └── 100_calibration.json      #   Ergebnisse aus dem Abgleich mit dem echten Spiel (§9)
└── schema/
    ├── tower.schema.json
    ├── round.schema.json
    └── patch.schema.json
```

**Fluss:**

```
Wiki-API ──tools/wiki/*.py──▶ raw/ ──tools/normalize.py──▶ base/ ──┐
                                                                   ├─▶ btd-data: laden, Patches anwenden, validate()
                                             patches/*.json ───────┘
```

- `raw/` wird bei einem Wiki-Update komplett neu erzeugt. Weil Korrekturen nie in `raw/` oder
  `base/` stehen, gehen sie dabei nicht verloren.
- `tools/normalize.py` (Python) übersetzt die vier unterschiedlichen Agent-Formate in `raw/`
  (Absolutwerte vs. Deltas, Angriffsnamen wie `"Attack/Projectile"`) in das strikte
  `base/`-Format. Quelle der Wahrheit für Stats sind die Wiki-Module
  `Module:BTD6 stats/<Tower>/new`. Was nicht übersetzt werden kann, landet in einem Report
  und wird über `patches/020_missing_values.json` nachgetragen.
- In `base/` steht pro Tier beides (Format: `data/schema/tower.schema.json`):
  - **`ops`**: die Änderungen dieses Tiers als `{target, stat, op: set|add|mul|remove, value}`,
    `target` = `"tower"` oder ein Angriffs-`tag`. Daraus erzeugt `btd-data` die `Modifier` aus §5.
    Nur so lassen sich Crosspaths (z. B. 3-2-0) zusammensetzen: aus zwei Absolutwerten ist nicht
    ablesbar, ob ein Upgrade addiert, multipliziert oder ersetzt. Wo das Wiki nur Absolutwerte
    liefert (Primary-Agent), sind die ops `set`. Für Crosspaths muss das später per Patch präzisiert werden.
  - **`state`**: der vollständige Zustand mit Absolutwerten. Er dient als geprüfter Snapshot für
    Tests (Modifier-Fold von `base` + `ops` muss `state` ergeben) und für Menschen zum Lesen.
- **Camo** gehört zum Turm (`state.camo`). Auf Angriffs-Ebene steht `camo: false` nur als explizite
  Ausnahme (Turm hat Camo, dieser Angriff nicht). Solche Fälle listet der Report.
- **`damage_vs`** nutzt ein festes Vokabular aus Bloon-Klassen (`moab bfb zomg ddt bad ceramic lead
  fortified camo purple boss`) und Status (`stunned frozen stickied glued shocked burning`).
  Zusammengesetzte Bedingungen werden mit `+` verbunden, z. B. `fortified+moab` = nur fortified MOABs.
- **`range` am Angriff** ist nur gesetzt, wenn er von der Turm-Range abweicht (`RangeOverride`).
- `tests/test_normalize.py` deckt jede Rohformat-Konvention ab und prüft, dass `data/base/` zum
  aktuellen `normalize.py` passt.

**Patch-Format** (angelehnt an Civs `<Update>`/`<Delete>`):

```json
{
  "patches": [
    { "op": "set",    "path": "towers/mortar_monkey/paths/bottom/0/ops/0/value", "value": false,
      "why": "Game-Data-Modul zeigt Camo erst ab 0-0-3, Text sagt 0-0-1" },
    { "op": "set",    "path": "towers/heli_pilot/footprint", "value": { "rect": [14, 14] },
      "why": "Wiki gibt Rechteck statt Radius" },
    { "op": "merge",  "path": "towers/dart_monkey/paths/top/2/ops/3", "value": { "op": "add", "value": 0.2 },
      "why": "Spike-o-pult: Cooldown +0.2 s statt fest 1.15 s (Crosspath mit Mittelpfad)" }
  ]
}
```

- `op`: `set`, `merge` (Objekt zusammenführen), `delete`.
- `path`: JSON-Pointer-ähnlich, relativ zu `base/`. Patches zielen auf **`ops`**, nicht auf
  `state`. Ein Patch auf ein Tier wirkt so automatisch auf alle späteren Tiers. `normalize.py`
  bzw. `btd-data` berechnet `state` danach neu.
- `why` ist Pflicht. Jede Korrektur ist damit nachvollziehbar, und `btd-tools validate-data`
  listet alle Patches mit Begründung.
- Ein Patch, dessen `path` ins Leere zeigt, ist ein **Fehler** (fängt ab, dass ein Wiki-Update
  die Struktur verschoben hat).
- Wenn wir in handgepflegten Dateien Kommentare brauchen, dürfen `patches/` auch als
  **RON** vorliegen (gleiche Struktur). `base/` und `raw/` bleiben JSON.

**Versionierung:** `base/` trägt pro Datei `game_version`. `btd-data` warnt, wenn Türme aus
verschiedenen Spielversionen gemischt werden.

### Typen

```rust
pub struct GameData {
    pub bloons: Vec<BloonDef>,          // Index = BloonKind
    pub towers: Vec<TowerDef>,          // Index = TowerKind
    pub rounds: Vec<RoundDef>,
    pub maps:   Vec<MapDef>,
}
```

### Bloons

- `BloonDef { name, hp, speed, rbe, children: SmallVec<[(BloonKind, u8); 2]>, immune: DamageTypes, classes: BloonClasses }`
- `BloonClasses` ist ein Bitset (`MOAB_CLASS`, `CERAMIC`, `LEAD`, …) für Schadensboni.
- `camo`, `fortified` sind **Flags auf der Instanz**, keine eigenen Typen (sonst explodiert
  die Typanzahl). Kinder erben Flags (wie im Spiel). **[KAL]**
- `regrow` braucht **Instanz-Zustand**, nicht nur ein Flag (s. §5 BloonStore).
- `RoundDef { groups: Vec<GroupDef> }`, `GroupDef { kind, flags, count, start: f32, end: f32 }`
  → Spawns gleichmäßig auf `[start, end]` verteilt. **[KAL]**

### Türme

- `TowerDef { name, cost, footprint, placement, base: TowerStats, attacks: Vec<AttackDef>, paths: [[UpgradeDef; 5]; 3] }`
- `AttackDef { tag: AttackTag, behavior, stats: AttackStats, … }`. Der `tag` (`"dart"`,
  `"shuriken"`, `"sticky_bomb"`, `"glaive"` …) ist das, worauf Upgrades zielen.
- `UpgradeDef { name, cost, mods: Vec<Modifier>, adds_attacks: Vec<AttackDef>, ability: Option<AbilityDef> }`

### Validierung & Konverter

- `validate()` beim Laden: Kinder existieren, Kosten > 0, jeder Modifier-Selektor trifft
  mindestens einen Angriff, keine `null`-Werte in Feldern, die der Core braucht.
  Fehlende Wiki-Werte ⇒ klarer Fehler mit Turm/Tier, keine stillen Defaults.
- `raw/` → `base/` übernimmt `tools/normalize.py` (s. oben). `btd-data` liest nur `base/` + `patches/`.

## 5. `btd-core`

### Module

```
core/
├── unity.rs        # Rundung & Mathf-Äquivalente (über libm)
├── money.rs        # Money(i64), Rundungsstellen
├── rng.rs          # Seed-Ableitung pro (Runde, Zweck)
├── map.rs          # Track (Polylinie, cum. Längen, Segmente), Platzierbarkeit, Footprint-Kollision
├── bloon.rs        # BloonStore (SoA), Schaden, Splitting, Regrow
├── stats.rs        # TowerStats/AttackStats, Modifier, Selektoren, Stat-Pipeline
├── effects.rs      # zeitlich begrenzte Effekte (Alchemist, Overclock, Abilities)
├── tower.rs        # TowerStore, Upgrades inkl. Crosspath-Regel, Verkauf
├── attack.rs       # Angriffs-Behaviors, Targeting
├── projectile.rs   # fliegende Projektile (ab M2)
├── spatial.rs      # Track-Buckets für Targeting
├── economy.rs      # Einkommen, Rundenbonus, Cash-per-Pop-Faktoren
├── spawner.rs      # RoundDef -> Spawns pro Tick
├── game.rs         # Game, Action, tick(), play_round()
└── events.rs       # optionales Event-Log für Replays/Debug
```

### Bloons: SoA

```rust
pub struct BloonStore {
    id: Vec<u32>, kind: Vec<BloonKind>, flags: Vec<BloonFlags>,
    hp: Vec<f32>, dist: Vec<f32>, speed_mul: Vec<f32>,   // Slow, Glue …
    seg: Vec<u16>,                // gecachter Track-Segmentindex (dist steigt monoton)
    regrow_max: Vec<BloonKind>,   // bis zu dieser Stufe wächst er nach (KindNone = kein Regrow)
    regrow_timer: Vec<u16>,       // Ticks bis zum nächsten Nachwachsen
    alive: Vec<bool>,
    // Status-Effekte später als eigene Vecs (frozen_ticks, burn …)
}
```

- **Bewegung:** ein Loop über `dist += speed * speed_mul * DT`, danach `seg` vorrücken,
  solange `dist > cum[seg+1]`. Keine Binärsuche. `pos(i)` interpoliert nur im gecachten Segment.
- **Entfernen:** im Tick nur `alive = false` markieren, am Tick-Ende **stabil kompaktieren**
  (ein O(n)-Durchlauf im `retain`-Stil über alle Spalten gleichzeitig). Reihenfolge bleibt
  erhalten, keine Sortierung nötig. Neue Kinder werden angehängt und bekommen neue IDs.
- **Schaden:** `damage(i, hit: &Hit)` prüft Immunität (Lead/Black/White/Purple/Camo),
  rechnet `hit.damage + hit.bonus_vs(classes, flags)`, zieht HP ab. Bei HP ≤ 0 spawnen die
  Kinder mit Überschuss-Schaden (rekursiv). Pops ⇒ `economy.on_pop()`.
- **Kinder-Spawns [KAL]:**
  - Versatz entlang des Tracks (`child_spacing` in `Rules`, Kinder leicht hinter dem Parent).
    Beeinflusst, wie viel Pierce wirkt.
  - Ob Kinder im selben Tick vom selben Angriff/Projektil getroffen werden dürfen.
    Standard: nein, jedes Projektil führt eine `hit_set` getroffener Bloon-IDs, und Kinder
    erben die Parent-ID in dieses Set. Per `Rules` umschaltbar.
- **Regrow:** pro Tick `regrow_timer -= 1`. Bei 0 wächst der Bloon eine Stufe (bis
  `regrow_max`) und der Timer wird zurückgesetzt. Kinder eines Regrow-Bloons erben `regrow_max`. **[KAL]**

### Stat-Pipeline (Modifier)

Stats sind in **Turm-Stats** und **Angriffs-Stats** getrennt:

```rust
pub enum TowerStat  { Range, CostMul, SellMul, Footprint }
pub enum AttackStat {
    Cooldown, Damage, Pierce, Projectiles, BlastRadius, ProjectileSpeed, Lifetime,
    RangeOverride,                 // falls ein Angriff eine eigene Range hat
    DamageVs(BonusClass),          // +X gegen Klasse/Status, s. unten
}
/// Entspricht den `damage_vs`-Schlüsseln in base/ ("fortified+moab" = beide Bedingungen).
pub struct BonusClass { pub all_of: SmallVec<[BonusTerm; 2]> }
pub enum BonusTerm {
    Class(BloonClass),   // Moab, Bfb, Zomg, Ddt, Bad, Ceramic, Lead, Fortified, Camo, Purple, Boss
    Status(Status),      // Stunned, Frozen, Stickied, Glued, Shocked, Burning
}
pub enum Flag { Camo, CanHitLead, CanHitBlack, CanHitWhite, CanHitPurple, CanHitFrozen, … }

pub struct Modifier { pub target: Selector, pub op: ModOp }

pub enum Selector {
    Tower,                 // Turm-Stats (Range, Kosten …)
    AllAttacks,
    ByTag(AttackTag),      // z.B. nur "shuriken", nicht "sticky_bomb"
}

pub enum ModOp {
    AddT(TowerStat, f32),  MulT(TowerStat, f32),  SetT(TowerStat, f32),
    AddA(AttackStat, f32), MulA(AttackStat, f32), SetA(AttackStat, f32),
    Flag(Flag, bool),
    DamageType(DamageType),
}
```

Effektive Stats eines Turms:

```
effective = fold(base,
                 upgrade_mods            // aus gekauften Tiers
              ++ aura_mods               // statische Buff-Quellen in Reichweite
              ++ timed_effects.mods)     // zeitlich begrenzte Effekte
```

- Reihenfolge innerhalb eines Stats: `Set` → `Add` → `Mul`. Dokumentiert und per Test
  fixiert. **[KAL]**
- **Zwei Buff-Arten:**
  1. **Statische Auren** (Village, Radar, Rabatt-Village): `BuffSource { radius, mods, stacks, filter }`.
     Werden gecacht und nur neu berechnet, wenn sich eine Quelle oder ein Ziel ändert (Bau,
     Upgrade, Verkauf in Reichweite ⇒ `dirty`).
  2. **Zeitlich begrenzte Effekte** (Alchemist-Brew, Overclock, Ability-Buffs):
     `timed_effects: SmallVec<[TimedEffect; 2]>` pro Turm,
     `TimedEffect { source, mods, ticks_left, charges: Option<u16> }`. Zählt jeden Tick
     herunter (oder pro Angriff bei `charges`, wie beim Alchemist). Beim Ablauf wird der
     Effekt entfernt und `dirty` gesetzt.
- Effektive Stats werden **gecacht**, neu berechnet nur bei `dirty`. Im Tick-Loop wird nichts
  gefoldet.

### Angriffe

```rust
pub enum Behavior {
    Hitscan    { },                            // Sniper
    Projectile { speed: f32, lifetime: f32 },  // Dart, Boomerang, Ninja (ab M2)
    Splash     { radius: f32 },                // Bomb, Mortar (Projektil + Explosion)
    Radial     { count: u8 },                  // Tack (Winkel über libm)
    Aura       { },                            // Ice
    Income     { per_round: Money },           // Farm
    Buff       (BuffSource),                   // Village
}
pub enum Targeting { First, Last, Close, Strong }

pub enum Reach {
    Radius,                 // normal: effektive Range des Turms/Angriffs
    Global,                 // Sniper: trifft überall, `range` ist nur der Anzeige-/Platzierungsbereich
    Reticle { pos: Vec2 },  // Mortar, Dartling: zielt auf einen gesetzten Punkt (Action::SetReticle)
    Flying,                 // Monkey Ace: fliegt eine Route, Angriffe relativ zum Flugzeug
    Follow,                 // Heli Pilot: folgt Ziel/Cursor, Angriffe relativ zum Heli
}
```

`Flying` und `Follow` sind in `base/` schon markiert, werden aber erst mit den jeweiligen
Türmen (nach M2) wirklich simuliert.

Die `range`-Werte im Wiki sind bei Sniper, Mortar und Dartling nur der kleine Anzeigebereich
im Spiel und nicht die Zielreichweite. Der Konverter setzt für diese Türme `Reach` explizit.
Für den RL-Agenten startet Mortar mit einem festen Reticle pro Map (vorberechnet auf den
dichtesten Track-Abschnitt), Dartling mit „First“ auf dem Track. `Action::SetReticle` kommt später.

**Targeting-Kosten:** naiv O(Türme × Bloons). Für frühe Runden reicht das. Für späte
Runden kommt `spatial.rs`: Der Track wird in Abschnitte fester Länge geteilt
(Buckets nach `dist`). Pro Turm wird **einmal beim Bau** vorberechnet, welche Abschnitte
seine Range schneidet. Pro Tick prüft er nur die Bloons in diesen Buckets. Bucket-Zuordnung
fällt beim Kompaktieren am Tick-Ende mit ab.

### Aktionen

```rust
pub enum Action {
    Place { kind: TowerKind, pos: Vec2 },
    Upgrade { tower: TowerId, path: u8 },
    Sell { tower: TowerId },
    SetTargeting { tower: TowerId, t: Targeting },
    UseAbility { tower: TowerId, ability: u8 },   // vorgesehen, Implementierung später
}
```

`can_apply` prüft:
- Cash (inkl. Schwierigkeits-Multiplikator und Rabatten, gerundet wie im Spiel **[KAL]**)
- Platzierung: Untergrund (Land/Wasser), Track-Abstand, **Footprint-Kollision** mit anderen Türmen
- **Crosspath-Regel:** höchstens zwei Pfade > 0, höchstens ein Pfad > 2
- Ability bereit (Cooldown abgelaufen)

### Öffentliche API

```rust
impl Game {
    pub fn new(data: Arc<GameData>, map: MapId, rules: Rules, seed: u64) -> Self;
    pub fn apply(&mut self, a: Action) -> Result<(), ActionError>;
    pub fn can_apply(&self, a: &Action) -> bool;
    pub fn play_round(&mut self) -> RoundReport;   // leaked, pops, cash, ticks
    pub fn tick(&mut self, n: u32);                // für Mid-Round-Aktionen/Abilities später
    pub fn state_hash(&self) -> u64;
}
#[derive(Clone)] pub struct Game { … }           // Clone billig => Suche/MCTS möglich
```

`Rules` enthält Schwierigkeit (Kosten-Multiplikator Easy/Medium/Hard), Start-Cash/-Leben,
Tick-Rate, Cash-per-Pop-Tabelle, Kinder-Spawn-Parameter und die
**Domain-Randomization-Konfiguration** (§7).

### Ziel-Modus: CHIMPS

`Rules::chimps()` ist das Preset, auf das wir hinarbeiten:

| Regel | Umsetzung |
|---|---|
| Schwierigkeit Hard | Kosten = Medium × 1.08, auf 5 $ gerundet **[KAL: Rundungsart]** |
| Runden 6–100 | Start in Runde 6 mit 650 $, Sieg nach Runde 100 |
| **C**ontinues: keine | Spielende bei Niederlage |
| **H**earts lost: keine | 1 Leben, jeder Leak = Niederlage |
| **I**ncome: nur Pops + Rundenbonus | Farmen/Einkommens-Upgrades sind gesperrt (nicht im Aktionsraum) |
| **M**onkey Knowledge: aus | gibt es bei uns ohnehin nicht |
| **P**owers: aus | gibt es bei uns ohnehin nicht |
| **S**elling: aus | `Action::Sell` ist maskiert |

Folgen für den Rest:
- Runden 6–100 brauchen den kompletten Bloon-Satz inkl. BFB, ZOMG, DDT und BAD
  sowie die MOAB-Health-Skalierung ab Runde 81 (aus den Wiki-Daten, §4).
- Heroes sind in CHIMPS erlaubt, kommen bei uns aber erst später. Ohne Hero ist es schwerer,
  aber machbar.
- **Zwischenziele vor CHIMPS:** Nur mit den 6 Primary-Türmen und ohne Hero ist CHIMPS sehr schwer.
  Benchmarks deshalb gestaffelt: (a) Primary-Türme auf Medium Standard (Runde 1–40, danach bis 60),
  (b) Primary-Türme auf Hard Standard (bis 80), (c) volles Turm-Set auf CHIMPS,
  (d) CHIMPS mit Hero (M6). Stufe (a)/(b) dienen vor allem als Test für Engine und Agent.
- **Training mit Curriculum** (in `btd-env`, nicht im Core):
  1. Leben-Puffer: erst mit vielen Leben und Leak-Strafe trainieren, den Puffer dann
     schrittweise auf 1 absenken (echtes CHIMPS).
  2. Rundenziel: erst bis 40, dann 60, 80, 100.
  3. Optional Start-Snapshots aus späteren Runden (z. B. aus guten eigenen Spielen), damit der
     Agent die späten Runden nicht nur extrem selten sieht.

## 6. `btd-env` + `btd-py`

- **Aktionsraum:** Grid-basiert (20×15 Zellen auf Monkey Meadow). Eine Zelle zeigt **nicht**
  auf ihren Mittelpunkt, sondern auf eine **geprüfte Weltposition**: Pro Map und
  Footprint-Größe wird vorab die gültige Position nächst am Zellmittelpunkt gesucht
  (oder die Zelle ist für diesen Turm gesperrt). Weil die Footprints dynamisch kollidieren,
  macht die Maske zur Laufzeit trotzdem `can_apply`. So bleibt der Weg zur Bridge
  (kontinuierliche Platzierung) offen.
- Aktionsraum wird aus `GameData` erzeugt und wächst automatisch mit der Turmanzahl.
- **`VecEnv` in Rust:** N Spiele, `step(actions: &[u32])`, füllt vorallozierte
  Observation- und Masken-Buffer, Rückgabe als `numpy`-Arrays ohne Kopie.
  GIL wird während `step` freigegeben, Spiele laufen via `rayon` parallel.
- Python-Seite: dünner `gymnasium.vector.VectorEnv`-Wrapper für sb3 / MaskablePPO.
- **Paritätstests als Pytest** in `btd-py/tests/`: dieselben Build-Orders in `btd` (Rust)
  und `btd_sim` (Python-Referenz), Rundenergebnisse vergleichen. So braucht `btd-tools`
  kein Python.

### Netz-Experimente (auf Benchmark (a), Runde 1–40)

Der leichte Benchmark ist auch das Testfeld für Netzarchitekturen. Vergleich immer mit
gleichem Budget an Env-Steps, mehreren Seeds (≥ 3) und den gleichen Kennzahlen:
erreichte Runde (Ø/Median), Sample-Effizienz (Runde über Steps) und Zeit pro Update.

| Variante | Idee |
|---|---|
| MLP-Größen | Baseline: `[512, 256]`, dann breiter/tiefer/schmaler |
| CNN über das Grid | Observation als Kanäle × 15 × 20 statt flach, Ortsbezug bleibt erhalten |
| Set/Attention über Türme | Türme als Liste von Feature-Vektoren (Typ, Tiers, Position), reihenfolge-unabhängig |
| Faktorisierte Action-Heads | erst Aktionstyp, dann Turmtyp, dann Position, statt eines flachen Discrete |
| Observation-Varianten | Track-Abstand pro Zelle; Runden-Vorschau und Bloon-Wissen (s. unten) |

**Runden-Vorschau und Bloon-Wissen** (wie ein guter Spieler, der die Runden kennt):

- **Vorschau-Fenster:** die nächsten *k* Runden (z. B. *k* = 5, zusätzlich zusammengefasst
  „bis Runde +20“), je Runde: Anzahl pro Bloon-Typ, Anteil Camo/Regrow/Fortified, RBE,
  max. MOAB-HP, Dauer und Dichte (Bloons pro Sekunde in der dichtesten Sekunde).
  Außerdem pro Spezial-Typ ein Zähler „Runden bis zum nächsten Auftreten“ (und ob es das erste
  Mal ist): Black, White, Zebra, Lead (Metall), Purple, Rainbow, Ceramic, MOAB, BFB, ZOMG,
  DDT, BAD sowie die Eigenschaften Camo, Regrow, Fortified. Das erste Auftreten ist der
  kritische Moment, auf den gute Spieler hinbauen.
- **Bloon-Tabelle:** statische Eigenschaften pro Typ (Speed, HP, RBE, Immunitäten,
  Kinder) als feste Embeddings bzw. Lookup im Netz, statt sie in jede Observation zu kopieren.
- Weil die Standard-Runden fix sind, könnte das Netz sie theoretisch auch über die
  Rundennummer auswendig lernen. Die explizite Vorschau lernt aber schneller und überträgt
  auf andere Rundensätze (Alternate Bloon Rounds, eigene Runden, Domain Randomization).
  Experiment: mit vs. ohne Vorschau.

`btd-env` liefert die Observation deshalb in mehreren Formen (flach, Grid-Tensor,
Turm-Liste), ausgewählt per Config. Die Netze selbst sind sb3-`features_extractor`s in Python.

## 7. Zufall & Domain Randomization

Konfigurierbar pro Spiel, aus dem `Noise`-Strom der jeweiligen Runde:

- Cooldowns, Bloon-Speed, Spawn-Zeiten: ±x % (gleichverteilt, pro Spiel bzw. pro Gruppe)
- Leichter Jitter auf Turm-Position und Reichweite
- Echte Spielmechaniken mit Zufall (Crit-Chance, Streuwinkel, zufällige Ziele) über `Combat`

Training mit Noise, Evaluation einmal ohne (deterministisch) und einmal mit.

## 8. Tests & Benchmarks

- **Unit:** Splitting/Überschuss-Schaden, Immunitäten, `DamageVs`, Modifier-Reihenfolge
  und Selektoren, Crosspath-Regel, Regrow, Banker's Rounding, Money-Rundung.
- **Determinismus:** gleicher Seed ⇒ gleicher `state_hash` (proptest über zufällige
  Aktionsfolgen), CI auf Linux, Windows und macOS. Replay ab Runde k aus Snapshot + Seed.
- **Parität vs. Python:** Pytest in `btd-py` (s. §6), solange die Features gleich sind.
- **Daten:** `btd-tools validate-data` in CI, bricht bei fehlenden Werten.
- **Bench (`criterion`), getrennt gemessen:**
  - **Simulierte Ticks/s** (pro Kern, nach Runde gestaffelt: frühe Runden, ~40, ~80)
  - **Env-Aktionen/s** (Bau/Upgrade-Schritte inkl. Maske + Observation, ohne Runde)
  - **Runden/s** als Gesamtgröße (das, was PPO wirklich bremst)

## 9. Kalibrierung gegen das echte Spiel

Alle **[KAL]**-Punkte auf einen Blick:

| Punkt | Default | Wie prüfen |
|---|---|---|
| Tick-Rate | 60/s | Cooldown-Messung per Video |
| Spawn-Verteilung in Gruppen | gleichmäßig | Spawn-Zeitpunkte einer Runde filmen |
| Kinder-Versatz / Same-Tick-Hits | kleiner Versatz / nein | Pierce-Test mit Dart gegen Ceramic-Gruppe |
| Flag-Vererbung (Camo/Fortified/Regrow) | erben | Wiki + Test |
| Regrow-Intervall | Wiki | Video |
| Modifier-Reihenfolge | Set→Add→Mul | Village + Upgrade-Kombi im Spiel ablesen |
| Rundungsstellen Cash / Rabatte | s. §3 | Kaufpreise auf Easy/Hard, Sell-Werte |
| Cash-Präzision (double?) | i64 milli-$ | Cash nach Runde 60+ vergleichen |
| Hard-Preise (×1.08, auf 5 $) | half-up auf 5 | Wiki-Preistabelle Hard vs. Medium für alle Upgrades vergleichen |

Die Screen-Capture-Bridge spielt feste Setups im echten BTD6, Leaks/Cash pro Runde werden mit
der Sim verglichen. Ergebnisse kommen als Testfälle nach `tests/calibration/`.

## 10. Meilensteine

1. **M1 – Parität (Pipeline-Test):** `btd-data` + `btd-core` mit dem Funktionsumfang der
   Python-Engine (4 Türme, Hitscan, prozedurale Runden), Paritätstests grün.
   `btd-env`/`btd-py` minimal, damit PPO einmal **durchläuft**. Kein ernsthaftes Training.
2. **M2 – Echte Physik & Daten:** Projektile mit Flugzeit, Wiki-Runden 1–40 inkl.
   Camo/Regrow/Fortified, Ceramic, MOAB; `tools/normalize.py` + Patches; Start-Türme = die klassischen
   Primary-Türme **Dart, Boomerang, Bomb, Tack, Ice, Glue** (ohne Desperado; Patch Notes 56/57
   für genau diese prüfen und Abweichungen als Patches eintragen);
   Modifier mit Selektoren und `DamageVs`.
3. **M3 – Ernsthaftes Training:** VecEnv mit rayon, Benchmarks, MaskablePPO auf Rust-Env
   mit Domain Randomization.
4. **M4 – Buffs & Support:** Village (Auren), Farm, Alchemist/Overclock (`timed_effects`),
   Abilities (`UseAbility`), Spatial-Buckets falls die Benchmarks es verlangen.
5. **M5 – Bridge:** Screen-Capture/Input für das echte Spiel, Kalibrierung (§9).
6. **M6 – Heroes (optional):** zuerst ein einfacher Hero als Turm mit Level-abhängigen Stats.
   Hero-XP/Leveling vorher recherchieren. Achtung: die Spalte „Base XP“ in der Wiki-Rundenliste
   ist (laut Nutzer) die Turm-XP zum Freischalten von Upgrades, nicht direkt die Hero-XP.

Begründung für Projektile in M2: Mit Hitscan lernt die Policy eine zu starke Physik
(besonders Darts gegen schnelle Bloons). Das gelernte Verhalten wäre nach dem Wechsel
wertlos.
