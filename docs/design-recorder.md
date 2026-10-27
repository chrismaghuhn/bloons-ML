# Design: Recorder-Mod und Abgleich Sim ↔ Spiel (Entwurf, Stand nach Feedback)

Ziel: Jede Partie im echten Spiel so mitschreiben, dass wir sie in der Sim exakt nachspielen und
**Tick für Tick** vergleichen können, statt aus Endständen (Leben, Screenshots) zurückzurechnen.
Erste Fragen, die damit beantwortet werden sollen:

1. Wo entstehen Bloons und wo verschwinden sie (Track-Abschnitte außerhalb des Bildes)? Wie schnell
   laufen sie wirklich?
2. Wann schießt ein Turm zum ersten Mal, in welchem Takt, auf wen (Sniper-Abweichung in Runde 1)?
3. Wann und womit leakt etwas, wie viele Leben kostet es?

Baut auf der vorhandenen Mod auf (`mod/BloonsMLMod`, Stufe 2 "Recorder" aus `mod/README.md`).

## 1. Was aufgezeichnet wird (zwei Dateien je Partie)

**a) Replay** im vorhandenen Format `btd-replay/2` (`engine/crates/btd-env/src/replay.rs`), damit
`btd-tools replay --compare` und `Env.load_replay` es direkt nutzen können:

- Aktionen mit Runde **und Frame** (im echten Spiel wird auch während der Runde gebaut): bauen
  (Turmname wie in game_v1, exakte Koordinaten), upgraden (Pfad), verkaufen. Das Replay-Format bekommt
  dafür ein optionales `frame` je Aktion (`btd-replay/3`, `/2` bleibt lesbar).
- Je Runde das Ergebnis: Leben, Cash (ganze $), Pops.
- Header: Karte, Modus, Schwierigkeit, Spielversion, **Mod-Version**; Targeting je Turm, falls nicht
  "First". Beim Vergleich hängt `trace-compare` Engine-Version und Hash der Sim-Daten an, damit nach
  einem Spiel-Update klar ist, welche Aufnahmen noch gelten.

**b) Trace** (`.trace.jsonl`, eine Zeile je Ereignis, Zeit in Sim-Frames des Spiels, 60/s, unabhängig
von Vorspulen):

| Ereignis | Felder |
|---|---|
| `round_start` / `round_end` | Runde, Frame |
| `spawn` | Bloon-ID, **Schlüssel** (Runde, Gruppe, Index in der Gruppe; Kinder: Schlüssel des Elternteils + Kind-Index, `parent_id`), Typ (inkl. Camo/Regrow/Fortified), Position, Distanz auf dem Pfad |
| `pos` (Stichprobe) | alle 6 Frames (0,1 s): je Bloon ID, Distanz auf dem Pfad, Position |
| `shot` | Turm-ID, Angriff/Waffe, Frame, Ziel-Bloon-ID, **was das Spiel gewürfelt hat** (Crit, Streuung, Ziel) |
| `tower` | beim Bauen/Upgraden die **effektiven** Werte: Range, Cooldown, Pierce, Damage, Projektiltempo (gegen den Konverter) |
| `cash` | Rohwert inklusive Nachkommastellen bei jeder Änderung (Rundungsstellen kalibrieren) |
| `damage` / `pop` | Bloon-ID, Schaden, neue Typen (Kinder), Quelle (Turm-ID) |
| `leak` | Bloon-ID, Typ, Frame, Distanz, verlorene Leben |

Die IDs von Spiel und Sim passen nicht zusammen; verglichen wird über den Schlüssel, der deshalb
schon in der Mod geschrieben wird (hinterher lässt er sich kaum rekonstruieren).

Stichproben: in Kalibrier-Partien **jeder Frame** (sonst verschwinden Sprünge/Glättung), in normalen
Partien alle 6 Frames (0,1 s). Dateien gzip-komprimiert (`.jsonl.gz`).

**Wann:** das Replay immer automatisch (billig), den Trace nur per Einstellung/Hotkey (schwer, für
gezielte Kalibrier-Partien).

## 2. Wie die Mod das abgreift (zu prüfen am echten Spiel)

- **Zuerst die Zeitbasis:** Zeit ist der interne Sim-Zähler bzw. die Sim-Zeit des Spiels, nicht
  Unity-Frames. Test: dieselbe Runde ohne Türme einmal in 1× und einmal vorgespult aufnehmen. Sind die
  Traces gleich, ist der Schritt fest; sonst `dt` je Schritt mitschreiben (dann ist nur ein
  statistischer Vergleich möglich).
- **Spawner prüfen:** Die Pfad-Polylinie hat die Mod schon gedumpt (sie reicht über den Bildrand);
  zusätzlich die Spawn-Distanz auf dem Pfad und das Spawner-Modell (`PathSpawnerModel`) auslesen.

- Mod Helper bietet Hooks für Runde, Turm (gebaut/Upgrade/verkauft), Bloon (erzeugt, Schaden, Pop,
  Leak). Was davon in 3.6.10 existiert und welche Felder ein Bloon/Turm hat (Pfad-Distanz, ID,
  Ziel), prüfe ich zuerst mit einer kleinen Test-Mod und dem MelonLoader-Log.
- Schüsse und die Frame-Zählung: Harmony-Patch auf die Waffen-/Simulationsschleife, falls Mod Helper
  dafür nichts hat.
- **Logging darf das Spiel nicht beeinflussen:** Ereignisse kommen in eine Queue, ein Hintergrundthread
  schreibt (kein Datei-IO auf dem Hauptthread).
- Steuerung: Replay automatisch, Trace per Einstellung/Hotkey, Ablage
  unter `<DumpRoot>/../recordings/<Datum>_<Karte>_<Modus>.{jsonl,trace.jsonl}` (lokal, gitignored).
- Wie bisher: nur offline / auf einem Profil spielen, bei dem ein Mod-Flag egal ist.

## 3. Gegenstück in der Sim

- `btd-core` bekommt einen optionalen Trace-Ausgang (`Option<&mut dyn TraceSink>` in `play_round`;
  ohne Sink keine Kosten). Der Sink zieht nie Zufall und ändert keine Reihenfolge; ein Test vergleicht
  `state_hash` mit und ohne Sink.
- **Engine-Umbau für Aktionen mitten in der Runde:** heute spielt `play_round` eine Runde am Stück.
  Nötig: Runde beginnen, n Ticks, Runde beenden, dazwischen Aktionen (für das Nachspielen echter
  Partien; die RL-Env kann beim Bauen zwischen den Runden bleiben).
- `btd-tools trace-compare spiel.jsonl spiel.trace.jsonl`: spielt das Replay in der Sim nach, schreibt
  den Sim-Trace und vergleicht je Runde: Spawn- und Leak-Distanz, Bloon-Tempo (Distanz über Zeit),
  erster Schuss und Takt je Turm, Leaks (wer, wann). Ausgabe als Tabelle plus Hinweise wie "Sniper:
  erster Schuss im Spiel 0,9 s später".
- **Divergenz eindämmen:** pro Runde neu ausrichten und je Runde die *erste* Abweichung melden, nicht
  die Folgefehler. Echte Zufallsmechaniken (Streuung, Crit, zufällige Ziele) statistisch vergleichen.
- Danach Korrekturen in Konverter/Engine, jede mit einem Test gegen eine echte Aufnahme.
- **Was ins Repo darf:** abgeleitete Zahlen (Tempo, Timings, Rundungsregeln, "erster Schuss nach X
  Frames") ja; Rohdaten (Pfad-Geometrie 1:1, Turmmodelle, Aufnahmen) bleiben lokal. Im Repo ein
  Skript, das die Patch-Werte aus dem lokalen Dump/den Aufnahmen erzeugt.

## 4. Reihenfolge

1. Test-Mod: Hooks und Felder, **Zeitbasis (1× gegen vorgespult)**, Spawner/Spawn-Distanz.
2. Recorder Teil a (Replay mit Frames) → mit `btd-tools replay --compare` nutzbar, solange nur zwischen
   den Runden gebaut wird.
3. Engine: Runde in Ticks + Aktionen mitten in der Runde; Sim-Trace mit Hash-Test.
4. Recorder Teil b (Trace) + `trace-compare`.
5. Kalibrier-Runde: Runde 1 ohne Türme, Sniper allein, Dart allein, Zwei-Turm-Build; Abweichungen
   beheben.

## 5. Entschieden

1. Replay immer automatisch, Trace per Einstellung/Hotkey.
2. Kalibrier-Partien: jeder Frame; normale Partien: 0,1 s; gzip.
3. Abgeleitete Zahlen ins Repo, Rohdaten lokal, Skript erzeugt die Patch-Werte.
