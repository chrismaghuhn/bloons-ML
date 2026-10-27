# Design: Bloons für CHIMPS (Regrow, Runden 81–100) — Entwurf, nicht freigegeben

Anlass: Die Sim kennt zwei Bloon-Mechaniken nicht, die CHIMPS deutlich schwerer machen. CHIMPS-Ergebnisse
der Sim sind darum zu optimistisch, ab Runde 81 vermutlich stark.

1. **Regrow** (nachwachsende Bloons): Das Datenformat lädt das Merkmal (`BloonDefV1.regrow`), die Engine
   ignoriert es. Grow-Blocker (Ice-Frost, Village, Druid) sind im Konverter als „ohne Wirkung“ markiert.
2. **Runden 81–100:** Im Spiel werden die Bloons ab Runde 81 stärker (siehe §3). Im Dump stehen dafür keine
   Werte; das ist Spiellogik.

Betroffene Runden mit Regrow-Bloons:
- Medium R60: 12 Runden (17, 19, 25, 29, 31, 36, 39, 42, 48, 49, 51, 59)
- CHIMPS zusätzlich: 61, 62, 67, 69, 70, 72, 74, 76, 79, 83, 90, 95
- sim_v0: keine, also bleiben Parität und Determinismus-Hash gleich

## 1. Befunde aus dem Dump (57.4)

- Jeder Regrow-Typ ist ein eigener Bloon (`RedRegrow`, `CeramicRegrowFortified` …). Seine Kinder sind
  wieder Regrow-Typen; das bildet der Konverter schon richtig ab.
- Das Verhalten steckt in `GrowModel { rate, growToId }`:
  - `rate` = **3,0 s** bei allen Typen.
  - `growToId` ist der nächstgrößere Typ: Red→Blue, Pink→Black, Zebra→Rainbow, Rainbow→Ceramic usw.
  - Lead, Purple und Ceramic haben kein Ziel; sie wachsen nicht weiter.
- **Obergrenze:** Ein Bloon wächst höchstens bis zu dem Typ zurück, als der er (oder sein Vorfahr) gespawnt
  ist. Das ist Laufzeitzustand des Spiels und steht nicht im Modell [KAL].
- **Grow-Blocker:**
  - Ice- und Druid-Frost: `GrowBlockModel_FreezeGrowBlock` am Projektil (blockiert, solange gefroren).
  - Village (u. a. 0-1-0): `GrowBlockModel` als Aura auf Bloons in Reichweite.

## 2. Regrow in der Sim

**Daten (game_v1):**
- `BloonDefV1`: neue Felder `grow_to: Option<String>` und `grow_rate: f64`. Der Konverter füllt sie aus
  `GrowModel`.
- Neue Felder bedeuten: Eine alte Erweiterung lädt neue Daten nicht (`deny_unknown_fields`), wie bei
  Village/Farm. Das kommt in die README.

**Engine (`BloonStore`):**
- Je Bloon zwei neue Werte:
  - `grow_cap`: der Spawntyp. Kinder erben ihn vom Elternteil.
  - `grow_timer`: Zeit seit dem letzten Wachsen (oder Spawn).
- Je Tick für Regrow-Bloons, wenn nichts blockiert und `grow_timer ≥ grow_rate`:
  - Liegt `grow_cap` auf der Kette ab `grow_to` (die Kette wird beim Laden je Typ vorberechnet), wird
    `kind = grow_to`.
  - Dabei wird `hp` auf den Wert des neuen Typs gesetzt und der Timer auf 0.
- Position und Status-Effekte (Frost, Glue, DoT) bleiben erhalten.
- Leak-Wert, Tempo und Immunitäten folgen dem neuen Typ (kommen ohnehin aus der Definition).

**Grow-Blocker:**
- Frost: Ein gefrorener Bloon wächst nicht (`frozen > 0`), wenn der Frost von einem Angriff mit
  FreezeGrowBlock kommt. Der Konverter setzt dafür ein Flag am Angriff.
- Village-Aura: Bloons in der Reichweite einer Village mit GrowBlock wachsen nicht. Das prüft derselbe Tick
  über die vorhandenen Auren-Zonen.
- Druid: über denselben Frost-Pfad.
- Im Konverter fallen damit die Einträge „ohne Wirkung“ weg; `cleanseRegen` (Regrow entfernen) bleibt
  vorerst `Unsupported`.

**Aufwand und Tempo:** Ein Vergleich und eine Addition je Regrow-Bloon und Tick; ohne Regrow-Bloons nichts.

## 3. Runden 81–100

Was ich über das Spiel weiß, aber im Dump nicht belegen kann (darum **alles [KAL]**):
- Ab Runde 81 werden Bloons schneller, mit jeder weiteren Runde ein Stück mehr.
- MOAB-Klassen bekommen mehr HP, mit jeder Runde mehr.
- Ceramics werden zu „Super Ceramics“ (mehr HP, andere Kinder).

**Messen mit dem Recorder (Mod-Session):**
- Partie im Sandbox-Modus auf Monkey Meadow, CHIMPS-Regeln, Runden 76–100 einzeln starten. Nur Türme, die
  wenig poppen, damit die Bloons den Pfad ganz durchlaufen.
- Der Recorder loggt je Bloon schon `health`, Typ, Distanz und `trackSpeedMultiplier`.
- Neu nötig: ein Ereignis bei **Typwechsel** (für Regrow-Zeiten) und die Anfangs-HP je Spawn.
- Daraus je Runde und Typ: Tempo (Distanz über Zeit), HP beim Spawn, Kinder (über `lineage`).

**Sim-Seite:** Die gemessenen Werte gehen als Tabelle in die Daten, `rules.round_scaling: [{ round,
speed_mul, moab_hp_mul, ceramic_hp }]`. Abgeleitete Zahlen dürfen ins Repo. Die Engine multipliziert sie beim
Spawn. Bis dahin gelten CHIMPS-Ergebnisse der Sim nur bis Runde 80.

**Regrow-Kalibrierung im selben Lauf:**
- Wächst ein Bloon sofort nach 3 s oder 3 s nach dem letzten Treffer?
- Regeneriert ein Regrow-Ceramic seine HP?
- Gibt ein nachgewachsener Layer beim erneuten Poppen wieder Geld?

## 4. Reihenfolge

1. Regrow in Daten und Engine mit Tests, ohne Blocker. Die Werte aus §1 sind belegt.
2. Grow-Blocker (Frost, Village, Druid).
3. Recorder: Typwechsel-Ereignis und Sandbox-Messung (Mod-Session; der User spielt die Messpartie).
4. Ab Runde 81: Skalierung aus den Messwerten, dann Regrow-Details aus der Messung nachziehen.

Nach Schritt 1/2 ändern sich die R60-Benchmarks (12 Runden). t12_r60_v2 und t14_r60 mit der neuen Sim neu
auswerten; dann entscheiden, ob neu trainiert wird.

## 5. Tests

- **Regrow-Kette:** Eine gespawnte RainbowRegrow, auf Zebra gepoppt, wächst nach 3 s wieder zu Rainbow und
  nicht weiter.
- **Obergrenze:** Eine als PinkRegrow gespawnte, auf Yellow gepoppte wächst nur bis Pink.
- **Blocker:**
  - Ein gefrorener Bloon wächst nicht.
  - In einer Village-Aura mit GrowBlock wächst er nicht.
- **Unverändert:**
  - Ohne Regrow-Bloons bleibt alles gleich (sim_v0-Hash, f64-Parität).
  - Determinismus mit Regrow (gleicher Seed, gleicher Hash).
- **Konverter:** `grow_to`/`grow_rate` für alle Regrow-Typen gesetzt, Ketten ohne Zyklen.

## 6. Offene Fragen an den User

1. Reihenfolge ok, Regrow zuerst, Runden ab 81 nach der Messung?
2. Weißt du aus dem Spiel, ob der Regrow-Timer bei einem Treffer neu startet und ob ein nachgewachsener
   Layer erneut Geld bringt? Sonst klären wir das mit dem Recorder.
3. Die Messpartie ab Runde 81 im Sandbox-Modus: Kannst du sie spielen, sobald die Mod das Typwechsel-Ereignis
   hat?
4. Nach Regrow die R60-Modelle nur neu auswerten oder gleich neu trainieren?
