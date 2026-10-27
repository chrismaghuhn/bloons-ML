# Design: mehr Türme, Schritt 1 (abgestimmt 2026-10-27; Teil 1 und 2 umgesetzt)

Ziel: von 6 auf 12 Türme (Sniper, Desperado, Super Monkey, Druid, Ninja, Wizard), mit Mechaniken
nah an dem, was die Engine schon kann. Buffs/Farm (Village, Banana Farm, Alchemist) sind Schritt 2
(M4), Luft- und Wassertürme kommen später bzw. gar nicht (Monkey Meadow hat kein Wasser).

## 1. Was die Basisvarianten brauchen (Dump 57.4)

| Turm | fehlende Modelle in 0-0-0 | geplante Abbildung |
|---|---|---|
| Sniper | InstantModel, InstantDamageEmissionModel, FilterAllExceptTargetModel, CheckTargetsWithoutOffsetsModel | neue Lieferart **Sofort-Treffer**: Schaden + Kontakt-Effekte direkt am Ziel, ohne Flug; Reichweite 9 999 999 = ganze Karte |
| Desperado | BurstWeaponBehaviorModel (2 Schüsse, 0,1 s), Instant…, TargetDesperado{First,Last,Close,Strong} | Sofort-Treffer + **Salve** (n Schüsse im Abstand t pro Angriff); Targeting nur "First" wie überall |
| Super Monkey | RandomArcEmissionModel (±8°), TowerRadiusModel | Streuung durch Mittelwert ersetzt (gerade); TowerRadius ist Darstellung, ignorieren |
| Druid | RandomEmissionModel (5 in 25°), OffsetModel, RemoveBloonModifiersModel (alle Schalter aus) | Zufallsfächer → gleichmäßiger Fächer; Offset ignorieren (wie Abwurf-Versatz); Remove… nur mit allen Schaltern aus annehmen |
| Ninja, Wizard | – (Basis geht) | Konverter: Upgrade-Dateien mit ":" im Namen heißen im Dump "_"; Namen in der Turmtabelle |

## 2. Neue Engine-Mechaniken (Entwurf; tatsächliche Umsetzung siehe "Stand" unten)

1. **Sofort-Treffer** (`Delivery::Instant`): trifft das gewählte Ziel im selben Tick, wendet Schaden,
   Extras (Filter, Status, Bonus-Schaden) und Kontakt-Effekte (z.B. Splitter/Explosion bei späteren
   Upgrades) an der Position des Ziels an. Pierce = Zahl der Ziele in Reihenfolge "First".
2. **Salve**: `burst: {count, interval}` am Angriff; nach dem Cooldown folgen `count` Schüsse im
   Abstand `interval` (Überschuss wird übertragen, wie beim Cooldown).
3. **Zielsuchende Projektile** (`TrackTargetModel`, `TrackTargetWithinTimeModel`): Projektil dreht
   pro Tick um höchstens `turnRate` auf sein Ziel (bzw. den nächsten "First"-Bloon in Reichweite,
   wenn das Ziel weg ist). Das ist der größte Brocken und schaltet vor allem Ninja- und
   Wizard-Upgrades frei; als eigener Teilschritt, damit die Basis-Türme vorher nutzbar sind.
4. **Zufall → Mittelwert**: RandomArcEmission/RandomEmission/RandomAngleOffset wie bisher Cluster-
   Reichweite und Crits (dokumentiert im Abdeckungs-Report, Kalibrierpunkt).

Alles Weitere bleibt gesperrt, wie bisher über den Abdeckungs-Report sichtbar (z.B. Abilities,
Wind/Blowback, Darkshift, Lightning, Life-based Attack Speed).

## 3. Konverter und Daten

- `tools/build_game_v1.py --towers primary|t12` (Standard bleibt `primary`, damit bestehende Modelle
  und Datendateien gültig bleiben); Ausgabe mit Suffix, z.B. `game_v1_tutorial_medium60_t12.json`.
- Abdeckungs-Report wie bisher je Turm.
- Mit 12 Türmen: Observation 2 + 12 + 3 = 17 Kanäle, 1 + (12 + 3 + 1) × 720 = 11 521 Aktionen.
  Bestehende Modelle passen nicht zu den neuen Daten (neue Läufe nötig).
- Sperren: `--ban-towers` bis 25 % der Türme, bei 12 also 3.

## 4. Tests und Abgleich

- Rust-Minitests je Mechanik (Sofort-Treffer trifft genau das Ziel und nur bis Pierce, Salve-Timing,
  Zielsuche dreht höchstens turnRate und trifft ein bewegtes Ziel).
- Konverter-Tests für die neuen Abbildungen.
- Plausibilität: `btd-tools demo` je neuem Turm (Pops je Runde), dazu ein kurzer Vergleich mit dem
  echten Spiel für Sniper und Desperado (DPS gegen eine bekannte Runde), wenn du Lust hast.
- Danach Lauf mit 12 Türmen (R60, Startzustände, Sperren bis 3) und dieselbe Auswertung (Vielfalt,
  Sperren, Erholung). Erwartung: mehr Vielfalt, weil es mehr gleichwertige Rezepte gibt.

## 5. Reihenfolge

1. Konverter-Fixes (Ninja/Wizard-Basis) + Sofort-Treffer + Salve + Mittelwert-Streuung → Sniper,
   Desperado, Super, Druid, Ninja, Wizard spielbar (je mit den Upgrades, die ohne Zielsuche gehen).
2. Zielsuchende Projektile → mehr Ninja-/Wizard-Upgrades.
3. Lauf + Auswertung.

## 6. Offene Fragen

1. Reihenfolge so, oder Zielsuche gleich mit in Teil 1?
2. Startzustände und Sperren im Lauf mit 12 Türmen beibehalten (Vorschlag: ja, Sperren bis 3)?
3. Willst du Sniper/Desperado im echten Spiel gegen eine Runde testen (Abgleich wie beim Sharp Shooter)?

## Stand Teil 1 (umgesetzt)

`--towers t12`: 256 von 768 Kombinationen (vorher 204 von 384). Neu: Sniper 26, Desperado 2, Super 9,
Druid 7, Ninja 3, Wizard 5 von je 64. Sofort-Treffer laufen über die vorhandene Lieferart `hitscan`
(Pierce 1 = nur das Ziel), Salve als `burst` am Angriff, zufällige Fächer als Mittelwert. Die neuen
Modelle sind "strikt": nur an der ausgewerteten Stelle erlaubt, sonst bleibt die Variante gesperrt.
Häufigste Lücken danach: Abilities (M4), Wind/Knockback, Zielsuche (Teil 2, v.a. Wizard), Desperado-
Eigenheiten (Vagrant/Standoff/Distanz-Bonus).

## Stand Teil 2 (Zielsuche)

`homing {turn_rate, distance, max_angle, retarget}` an geraden Projektilen: Startziel = Ziel des Turms;
ist es weg oder schon getroffen, mit `retarget` der nächste anvisierbare Bloon im Suchradius und im
Kegel um die Flugrichtung; Drehung höchstens `turn_rate` pro Sekunde. TrackTargetWithinTime (Ninja):
Drehung bis zur Kegelkante in `timeInFrames`. [KAL: Zielwahl beim Neu-Anvisieren, Kegel = voller
Öffnungswinkel.] Damit 275 von 768 Kombinationen (Wizard 21, Ninja 6).

## Kalibrierpunkte aus Schritt 1

- Salve: Cooldown startet mit dem ersten Schuss; ohne Ziel verfällt der Rest der Salve.
- OffsetModel (Druid, Sniper-Supply-Drop) ignoriert; Bedeutung unbestätigt (evtl. Zufallsdrehung).
- RandomEmission-`angle` als voller Fächer; RandomArc/RandomEmission durch Mittelwert ersetzt.
- Zielsuche: Zielwahl beim Neu-Anvisieren (nächster Bloon), Kegel = voller Öffnungswinkel.
- Immunitäts-Bit 64 in `immuneBloonProperties` (z.B. 72/73 bei Super 2xx, Wizard, Druid) wird vom
  Konverter ignoriert; Bedeutung im Spiel prüfen.

## Zufall mit Seed (2026-10-27)

`Crit.every_range` (aus `lower`/`upper` des Dumps) und `Game::set_seed`: mit Zufall wird der Abstand
bis zum nächsten Crit je Crit gewürfelt, sonst gilt `every` (bitgleich wie zuvor). Env:
`EnvConfig.randomness` / `train.py --randomness`, je Episode ein Seed aus dem Env-Zufall. Laut Dump
57.4 ist der Abstand nur beim Super Monkey zufällig (mittlerer Pfad: x3x/x4x 15-20, x5x 13-17); Sharp Shooter (10) und
Crossbow Master (5) sind fest. Weitere Mittelwert-Stellen (Cluster-Reichweite, Zufallsfächer) können
später an denselben Zufall. Aufzeichnen (`record`) und Zufall zusammen sind vorerst gesperrt, weil
Replays den Seed nicht speichern. [KAL: ob das Spiel `upper` einschließt; `every` ist der gerundete
Mittelwert, bei 15-20 also 18 statt 17,5.]
