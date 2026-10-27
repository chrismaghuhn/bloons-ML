# Design: Bloons für CHIMPS (Regrow, Runden 81–100) — freigegeben, Schritt 1 umgesetzt

Anlass: Die Sim kennt zwei Bloon-Mechaniken nicht, die CHIMPS deutlich schwerer machen. CHIMPS-Ergebnisse
der Sim sind darum zu optimistisch, ab Runde 81 vermutlich stark.

1. **Regrow** (nachwachsende Bloons): Das Datenformat lädt das Merkmal (`BloonDefV1.regrow`), die Engine
   ignoriert es. Grow-Blocker (Ice-Frost, Village, Druid) sind im Konverter als „ohne Wirkung“ markiert.
2. **Runden 81–100:** Im Spiel werden die Bloons ab Runde 81 stärker (siehe §3). Im Dump stehen dafür keine
   Werte; das ist Spiellogik.

Betroffene Runden mit Regrow-Bloons:
- Medium R60: 12 Runden (17, 19, 25, 29, 31, 36, 39, 42, 48, 49, 51, 59)
- CHIMPS zusätzlich: 61, 62, 67, 69, 70, 72, 74, 76, 79, 83, 90, 95
- dazu Runden mit DDTs (93, 99, 100 über den BAD): DDTs wachsen nicht, ihre Ceramic-Kinder sind aber
  Regrow-Typen
- sim_v0: keine, also bleiben Parität und Determinismus-Hash gleich

## 1. Befunde aus dem Dump (57.4)

- Jeder Regrow-Typ ist ein eigener Bloon (`RedRegrow`, `CeramicRegrowFortified` …), 26 insgesamt (normal,
  Camo, Fortified). Seine Kinder sind wieder Regrow-Typen; das bildet der Konverter schon richtig ab.
- `GrowModel { rate, growToId, overrideChildWithColor }`:
  - `rate` = **3,0 s** bei allen Typen.
  - `growToId`: der nächstgrößere Typ. Kette Red→Blue→Green→Yellow→Pink→Black→Zebra→Rainbow→Ceramic, dazu
    White→Zebra. Camo wächst zu Camo. Lead, Purple und Ceramic haben kein Ziel (`""` oder `null`).
  - `overrideChildWithColor` bei White (Pink), Purple (Pink) und Lead (Black): Diese Kinder wachsen zu
    ihrem **Elternteil** zurück (Pink→White, Pink→Purple, Black→Lead), nicht entlang `growToId`.
  - Fortified: CeramicRegrowFortified zerplatzt in RainbowRegrow, deren Ziel ist das **nicht** befestigte
    CeramicRegrow. Ob das Spiel beim Nachwachsen wieder befestigt, ist [KAL].
- **Obergrenze:** Ein Bloon wächst höchstens bis zu dem Typ zurück, als der er (oder sein Regrow-Vorfahr)
  gespawnt ist. Das ist Laufzeitzustand des Spiels und steht nicht im Modell [KAL].
- **Bloons ohne Regrow mit Regrow-Kindern:** DDTs (alle Varianten) zerplatzen in CeramicRegrow-Varianten.
- **Grow-Blocker:**
  - Jedes `FreezeModel` im Dump trägt ein `growBlockModel` (Ice, Druid 4xx/5xx, Engineer 4xx, …): Gefrorene
    Bloons wachsen nicht.
  - Village: `GrowBlockModel` als Zone auf Bloons mit Tag Grow, Radius = Turmreichweite, in allen Tiers mit
    Mittelpfad ≥ 1.

## 2. Regrow in der Sim

**Daten (game_v1):**
- `BloonDefV1`: neues Feld `grow_rate: f64` (0 = wächst nicht). Der Konverter liest es aus `GrowModel`.
- `growToId` und `overrideChildWithColor` braucht die Engine nicht. Der Konverter prüft damit nur, dass jeder
  Schritt Elternteil→Kind zum Spiel passt (`""` und `null` = kein Ziel).
- Neue Felder bedeuten: Eine alte Erweiterung lädt neue Daten nicht (`deny_unknown_fields`), wie bei
  Village/Farm. Das kommt in die README.

**Engine (`BloonStore`), je Bloon zwei neue Werte:**
- `grow_stack`: die Regrow-Vorfahren des Bloons, höchstens 9 Typen (`[BloonKind; 9]` plus Länge), der
  jüngste oben.
- `grow_timer`: Zeit seit Spawn oder letztem Wachsen; Start 0.

**Beim Spawn (Runde oder Kind):**
- Rundenspawn: leerer Stapel.
- Kind eines Regrow-Bloons: der Stapel des Elternteils, dazu der Elternteil selbst und alle Zwischentypen
  bis zum Kind. Dafür liefert
  `bloon::hit()` bei Überschuss-Schaden über mehrere Ebenen je Kind den ganzen Pfad. Heute gibt es nur die
  letzten Kinder zurück.
- Kind eines Bloons ohne Regrow (z.B. Ceramic-Kinder eines DDT): leerer Stapel, wächst also höchstens bis
  zum eigenen Typ zurück.

Der Stapel deckt alle Fälle aus §1 ab, ohne Sonderregeln:
- Obergrenze: Wächst der Bloon zurück, landet er beim Spawntyp, dann ist der Stapel leer.
- Override: Pink aus White, Purple oder Lead wächst zum Elternteil zurück, weil der oben im Stapel liegt.
- Überschuss-Schaden: Lead→Black→Pink wächst über Black zurück zu Lead und hört dort auf.

**Je Tick** (in der Bewegungsschleife, gefrorene Bloons sind dort schon ausgenommen; der Timer hält also
beim Einfrieren an [KAL]): Für Regrow-Bloons, nicht in einer Grow-Block-Zone, mit nicht leerem Stapel und
`grow_timer ≥ grow_rate`:
- `kind = grow_stack.pop()`, `hp` = HP dieses Typs, Timer auf 0.
- Position, ID und Status-Effekte bleiben. Die gleiche ID heißt: Ein Projektil, das ihn schon getroffen
  hat, trifft ihn nicht noch einmal.
- **Fortified [KAL]:** Mit dem Stapel wachsen die Kinder eines CeramicRegrowFortified wieder zum
  *befestigten* Ceramic (20 HP). Laut `growToId` wäre es das unbefestigte (10 HP). Wir nehmen den Stapel;
  die Messung entscheidet.

**Was aus dem Typ folgt:**
- Leak-Wert, Tempo und Immunitäten kommen aus der Definition des neuen Typs.
- Pops eines nachgewachsenen Layers geben wieder Geld. So zählt die Sim Pops schon heute [KAL].
- `Flags::REGROW` am einzelnen Bloon bleibt ohne Wirkung; Regrow hängt am Typ.

**Grow-Blocker:**
- Frost: Ein gefrorener Bloon wächst nicht. Das ergibt sich aus `frozen > 0`, ein eigenes Flag braucht es
  nicht.
- Village: über den vorhandenen Bloon-Zonen-Weg (`Zone`/`ZoneRt`) mit neuem `grow_block: bool`
  (`#[serde(default)]`, `speed_scale` 1), Filter Tag Grow. Die Village spielt im CHIMPS-Lauf bisher keine
  Rolle; deshalb kommt sie zuletzt.
- Im Konverter fallen damit die Einträge „ohne Wirkung“ weg; `cleanseRegen` (Regrow entfernen) bleibt
  vorerst `Unsupported`.

**Aufwand und Tempo:** Ein Vergleich und eine Addition je Regrow-Bloon und Tick; ohne Regrow-Bloons nichts.
Speicher: 9 Typen plus Länge mehr je Bloon.

## 3. Runden 81–100

Was ich über das Spiel weiß, aber im Dump nicht belegen kann (darum **alles [KAL]**):
- Ab Runde 81 werden Bloons schneller, mit jeder weiteren Runde ein Stück mehr.
- MOAB-Klassen bekommen mehr HP, mit jeder Runde mehr.
- Ceramics werden zu „Super Ceramics“ (mehr HP, andere Kinder).

**Messen mit dem Recorder (Mod-Session):**
- Partie im Sandbox-Modus auf Monkey Meadow, CHIMPS-Regeln, Runden 76–100 einzeln starten. Nur Türme, die
  wenig poppen, damit die Bloons den Pfad ganz durchlaufen.
- Der Recorder loggt je Schritt nur ID, Distanz und Position. Typ und `health` stehen bei `spawn`,
  `bloon_info` (dort auch die Anfangs-HP) und `leak`.
- Neu nötig:
  - ein Ereignis bei **Typwechsel** (für Regrow-Zeiten),
  - ein Schaden-/Pop-Hook (`Bloon.Damage`) und `cash`-Ereignisse (Timer nach Treffer, Geld für
    nachgewachsene Layer).
- Beim Auswerten den Tempofaktor der Schwierigkeit (Hard/CHIMPS ×1,25) herausrechnen. Ob die Sandbox
  CHIMPS-Regeln annimmt, ist offen; sonst Hard.
- Daraus je Runde und Typ: Tempo (Distanz über Zeit), HP beim Spawn, Kinder (über `lineage`).

**Sim-Seite:** Die gemessenen Werte gehen als Tabelle in die Daten, `rules.round_scaling: [{ round,
speed_mul, moab_hp_mul, ceramic_hp }]`. Abgeleitete Zahlen dürfen ins Repo. Die Engine multipliziert sie beim
Spawn. Bis dahin gelten CHIMPS-Ergebnisse der Sim nur bis Runde 80.

**Regrow-Kalibrierung im selben Lauf:**
- Wächst ein Bloon sofort nach 3 s oder 3 s nach dem letzten Treffer? Hält der Timer beim Einfrieren an?
- Ist ein nachgewachsenes Ceramic bzw. Lead wieder befestigt, und mit welchen HP startet es?
- Regeneriert ein Regrow-Ceramic seine HP?
- Gibt ein nachgewachsener Layer beim erneuten Poppen wieder Geld?

## 4. Reihenfolge

1. Regrow in Daten und Engine mit Tests, dazu der Frost-Blocker (fällt aus `frozen > 0` ab). Die Werte aus
   §1 sind belegt.
2. Village-Grow-Blocker (Bloon-Zone).
3. Recorder: Typwechsel-Ereignis und Sandbox-Messung (Mod-Session; der User spielt die Messpartie).
4. Ab Runde 81: Skalierung aus den Messwerten, dann Regrow-Details aus der Messung nachziehen.

Nach Schritt 1/2 ändern sich die R60-Benchmarks (12 Runden). t12_r60_v2 und t14_r60 mit der neuen Sim neu
auswerten; dann entscheiden, ob neu trainiert wird.

## 5. Tests

- **Regrow-Kette:** Eine gespawnte RainbowRegrow, auf Zebra gepoppt, wächst nach 3 s wieder zu Rainbow und
  nicht weiter.
- **Obergrenze:** Eine als PinkRegrow gespawnte, auf Yellow gepoppte wächst nur bis Pink.
- **Zebra:** Ein ZebraRegrow zerplatzt in Black und White; beide wachsen zu Zebra, aus einem werden zwei.
- **Override:** Pink aus White, Purple und Lead wächst zu White, Purple bzw. Lead zurück.
- **Überschuss-Schaden mit Override:** Lead bis Pink gepoppt wächst über Black nur bis Lead; Purple bzw.
  White bis Yellow gepoppt wächst nur bis Purple bzw. White.
- **Fortified:** Ein CeramicRegrowFortified wächst nach dem Zerplatzen wieder zum befestigten Ceramic (Stapel; [KAL]).
- **DDT-Kinder:** Die Ceramic-Kinder eines DDT wachsen nur bis Ceramic.
- **Überschuss-Schaden:** über mehrere Ebenen enthält der Stapel alle Zwischentypen.
- **Blocker:**
  - Ein gefrorener Bloon wächst nicht.
  - In einer Village-Aura mit GrowBlock wächst er nicht.
- **Unverändert:**
  - Ohne Regrow-Bloons bleibt alles gleich (sim_v0-Hash, f64-Parität).
  - Determinismus mit Regrow (gleicher Seed, gleicher Hash).
- **Konverter:** `grow_rate` für alle Regrow-Typen gesetzt; jeder Schritt Elternteil→Kind passt zu `growToId`
  bzw. `overrideChildWithColor` (Vergleich ohne Fortified, sonst scheitert CeramicRegrowFortified→Rainbow).

## 6. Offene Fragen an den User

1. Reihenfolge ok, Regrow zuerst, Runden ab 81 nach der Messung?
2. Weißt du aus dem Spiel, ob der Regrow-Timer bei einem Treffer neu startet und ob ein nachgewachsener
   Layer erneut Geld bringt? Sonst klären wir das mit dem Recorder.
3. Die Messpartie ab Runde 81 im Sandbox-Modus: Kannst du sie spielen, sobald die Mod das Typwechsel-Ereignis
   hat?
4. Nach Regrow die R60-Modelle nur neu auswerten oder gleich neu trainieren?
