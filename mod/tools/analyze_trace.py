"""Auswertung der Test-Recorder-Traces (docs/design-recorder.md, Schritt 1).

    .venv/bin/python mod/tools/analyze_trace.py TRACE [TRACE2]

Mit einer Datei: Zeitbasis (Sim-Schritte je Unity-Frame, Zählerverlauf, dt/timeScale), Pfade (spawnDist/leakDist),
Spawn- und Leak-Distanz, Laufzeit je Bloon in Sim-Schritten und Sekunden (60 Schritte/s).
Mit zwei Dateien (z.B. Runde 1 in 1× und vorgespult): vergleicht je Bloon (Schlüssel: Runde aus `round_start`,
Rundenzeit beim Erzeugen, Nr., Abstammung) die Distanz zum selben Rundenzeitpunkt und meldet die größte Abweichung.
Liest nur; die Traces liegen lokal unter data/raw/recordings/ (gitignored).
"""
from __future__ import annotations

import argparse
import gzip
import json
from collections import Counter, defaultdict
from pathlib import Path

STEPS_PER_SECOND = 60


def load(path: Path) -> list[dict]:
    """Liest auch abgeschnittene Dateien (Spiel abgestürzt/beendet, bevor gzip abgeschlossen war)."""
    opener = gzip.open if path.suffix == ".gz" else open
    events: list[dict] = []
    try:
        with opener(path, "rt", encoding="utf-8") as f:
            for line in f:
                if line.strip():
                    events.append(json.loads(line))
    except (EOFError, gzip.BadGzipFile, json.JSONDecodeError) as e:
        print(f"  Hinweis: {path.name} unvollständig ({type(e).__name__}), {len(events)} Zeilen gelesen")
    return events


def bloon_keys(events: list[dict]) -> dict[int, tuple]:
    """Spiel-ID -> Schlüssel (Runde, createdAtRoundTime, Nr., Kind-Nr. je Elternteil …), unabhängig von IDs.
    Die Runde kommt aus dem letzten `round_start` (Bloon.spawnRound ist in 57.4 nicht die Rundennummer, meist 0).
    Bloons mit mehrdeutigem Schlüssel werden weggelassen (und gemeldet)."""
    keys: dict[int, tuple] = {}
    children: Counter = Counter()
    same_time: Counter = Counter()
    round_no = None
    for e in events:
        if e["ev"] == "round_start":
            round_no = e.get("round")
            continue
        if e["ev"] != "spawn":
            continue
        lineage = e.get("lineage") or []
        parent = lineage[-1] if lineage and lineage[-1] != e["id"] else None
        if parent is not None and parent in keys:
            children[parent] += 1
            keys[e["id"]] = keys[parent] + (children[parent],)
        else:
            # emissionIndex ist im Spiel (57.4) für alle Bloons einer Runde 0; die Rundenzeit beim Erzeugen
            # unterscheidet sie, gleichzeitig erzeugte zählen in Reihenfolge
            base = (round_no, e.get("createdAtRoundTime"))
            same_time[base] += 1
            keys[e["id"]] = base + (same_time[base],)
    counts = Counter(keys.values())
    dupes = {k for k, n in counts.items() if n > 1}
    if dupes:
        print(f"  Hinweis: {len(dupes)} mehrdeutige Bloon-Schlüssel (z.B. {sorted(dupes)[:3]}), nicht verglichen")
    return {bid: key for bid, key in keys.items() if key not in dupes}


def summarize(path: Path, events: list[dict]) -> dict:
    header = next((e for e in events if e["ev"] == "header"), {})
    steps = [e for e in events if e["ev"] == "step"]
    print(f"\n== {path.name}")
    print(f"  Karte {header.get('map')} {header.get('mode')}/{header.get('difficulty')}, Spiel {header.get('gameVersion')}, "
          f"Positionen alle {header.get('positionEverySteps')} Schritte")
    for p in header.get("paths", []):
        print(f"  Pfad {p['index']} ({p.get('pathId')}): spawnDist {p['spawnDist']}, leakDist {p['leakDist']}, "
              f"maxUntilSpawn {p['maximumDistanceUntilSpawn']}, maxUntilLeak {p['maximumDistanceUntilLeak']}, "
              f"{p['segments']} Segmente")
    if steps:
        dts = Counter(b["t"] - a["t"] for a, b in zip(steps, steps[1:]))
        per_frame = Counter()
        frames = defaultdict(int)
        for s in steps:
            frames[(s["uf"], s["ff"])] += 1
        for (_, ff), n in frames.items():
            per_frame[(ff, n)] += 1
        print(f"  {len(steps)} Sim-Schritte, Zähler t {steps[0]['t']}..{steps[-1]['t']}, Zuwachs je Schritt: {dict(dts)}")
        print("  Sim-Schritte je Unity-Frame (ff, Schritte): "
              + ", ".join(f"{k}: {v}x" for k, v in sorted(per_frame.items())))
        ts = Counter((s["ff"], s["ts"]) for s in steps)
        print(f"  timeScale (ff, ts): {dict(ts)}")

    spawns = {e["id"]: e for e in events if e["ev"] == "spawn"}
    leaks = {e["id"]: e for e in events if e["ev"] == "leak"}
    if spawns:
        dists = Counter(round(e["dist"], 3) for e in spawns.values())
        print(f"  Spawns: {len(spawns)}, Spawn-Distanzen (beim Erzeugen) {dict(dists)}, "
              f"Typen {dict(Counter(e['type'] for e in spawns.values()))}")
        first_seen: dict[int, tuple] = {}
        for s in steps:
            for bid, _path, dist, x, y in s.get("b", []):
                first_seen.setdefault(bid, (s["rt"], dist, x, y))
        firsts = [first_seen[i] for i in spawns if i in first_seen]
        if firsts:
            fd = Counter(round(d, 3) for _, d, _, _ in firsts)
            fx = Counter((round(x, 1), round(y, 1)) for _, _, x, y in firsts)
            print(f"  Erste Position im Trace: Distanzen {dict(fd)}, Punkte {dict(fx.most_common(3))}")
    if leaks:
        dists = Counter(round(e["dist"], 1) for e in leaks.values())
        times = [leaks[i]["t"] - spawns[i]["t"] for i in leaks if i in spawns]
        print(f"  Leaks: {len(leaks)}, Leak-Distanzen {dict(dists)}")
        if times:
            print(f"  Laufzeit Spawn→Leak: {min(times)}..{max(times)} Schritte = "
                  f"{min(times) / STEPS_PER_SECOND:.2f}..{max(times) / STEPS_PER_SECOND:.2f} s")
    rounds = [e for e in events if e["ev"] in ("round_start", "round_end")]
    for e in rounds:
        print(f"  {e['ev']} Runde {e.get('round')} bei t {e.get('t')} (Rundenzeit {e.get('rt')})")
    starts = [e for e in rounds if e["ev"] == "round_start"]
    ends = [e for e in rounds if e["ev"] == "round_end"]
    if starts and ends:
        d = ends[0]["t"] - starts[0]["t"]
        print(f"  Rundendauer (erste Runde): {d} Schritte = {d / STEPS_PER_SECOND:.2f} s")
    leaked_at = {e["id"]: e["rt"] for e in events if e["ev"] == "leak"}
    return {"steps": steps, "keys": bloon_keys(events), "spawns": spawns, "leaked_at": leaked_at}


def positions_by_key(info: dict) -> dict[tuple, dict[int, float]]:
    """Schlüssel -> {Rundenzeit: Distanz}. Nach dem Leak bleiben Bloons noch einige Schritte mit eingefrorener
    Distanz in der Liste; diese Punkte werden verworfen."""
    out: dict[tuple, dict[int, float]] = defaultdict(dict)
    leaked_at = info["leaked_at"]
    for s in info["steps"]:
        for bid, _path, dist, _x, _y in s.get("b", []):
            key = info["keys"].get(bid)
            if key is not None and s["rt"] <= leaked_at.get(bid, s["rt"]):
                out[key][s["rt"]] = dist
    return out


def interpolate(series: list[tuple[int, float]], rt: int) -> float | None:
    """Distanz zur Rundenzeit rt, linear zwischen den Nachbarpunkten (None außerhalb)."""
    import bisect
    times = [t for t, _ in series]
    i = bisect.bisect_left(times, rt)
    if i < len(series) and times[i] == rt:
        return series[i][1]
    if i == 0 or i == len(series):
        return None
    (t0, d0), (t1, d1) = series[i - 1], series[i]
    return d0 + (d1 - d0) * (rt - t0) / (t1 - t0)


def compare(a: dict, b: dict) -> None:
    pa, pb = positions_by_key(a), positions_by_key(b)
    common = set(pa) & set(pb)
    print(f"\n== Vergleich: {len(common)} gemeinsame Bloons (nur in A: {len(set(pa) - common)}, nur in B: {len(set(pb) - common)})")
    worst = (0.0, None, None)
    compared = exact = 0
    for key in common:
        series_b = sorted(pb[key].items())
        for rt, dist in pa[key].items():
            exact += rt in pb[key]
            other = interpolate(series_b, rt)
            if other is None:
                continue
            compared += 1
            diff = abs(dist - other)
            if diff > worst[0]:
                worst = (diff, key, rt)
    print(f"  {compared} Vergleichspunkte (gleicher Bloon, gleiche Rundenzeit), davon {exact} ohne Interpolation")
    if compared == 0:
        print("  keine Vergleichspunkte (Positionen fehlen oder Schlüssel passen nicht)")
    elif worst[1] is None:
        print("  maximale Distanz-Abweichung: 0 -> Schritt fest, Traces identisch")
    else:
        print(f"  maximale Distanz-Abweichung: {worst[0]:.6f} bei Bloon {worst[1]}, Rundenzeit {worst[2]}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("traces", type=Path, nargs="+")
    args = ap.parse_args()
    infos = [summarize(p, load(p)) for p in args.traces]
    if len(infos) == 2:
        compare(*infos)


if __name__ == "__main__":
    main()
