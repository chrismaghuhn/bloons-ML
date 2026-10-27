"""Vielfalt und Robustheit einer Policy messen (Kriterium: docs/m3-log.md, "Festhäng-Metrik").

    python tools/analyze_policy.py models/r60_bans --data data/local/game_v1_tutorial_medium60.json \\
        --preview 3 --ban-towers 2

Teil 1 (Vielfalt): Spiele ohne Sperre, stochastische Policy. Je Episode der Schlüssel
(erster Turmtyp, {(Turmtyp, Haupt-Pfad)}); ausgegeben werden die Entropie H dieser Schlüssel und
Verteilungen von erstem Turm, Haupt-Pfaden, Runde des ersten Turms je Typ und belegten Zellen.

Teil 3 (Erholung, mit `--recovery DATEI`): aus jedem Zustand des festen Eval-Satzes weiterspielen
(tools/make_recovery_set.py); Siege und erreichte Runde, deterministisch und stochastisch.

Teil 2 (Robustheit): jede Sperr-Kombination mit 0, 1 und 2 gesperrten Türmen, deterministisch
(erreichte Runde, Leben) und stochastisch (Ø Runde, Siege).

`--ban-towers` muss zum Training passen (Observation); Modelle ohne Sperren-Training werden mit 0
ausgewertet, die Sperren wirken dann nur über die Maske.
"""
from __future__ import annotations

import argparse
import itertools
import json
import math
import sys
from collections import Counter
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from btd_sim.rust_env import RustBTDEnv  # noqa: E402

PATHS = "abc"


def _plain(model, obs, mask, deterministic):
    return model.predict(obs, action_masks=mask, deterministic=deterministic)[0]


PREDICT = _plain  # mit --lookahead ersetzt  # Pfad 0/1/2 als a/b/c in den Schlüsseln


def play(env, model, banned, deterministic):
    obs, info = env.reset(options={"banned": banned})
    done = False
    while not done:
        a = PREDICT(model, obs, env.action_masks(), deterministic)
        obs, _, done, _, info = env.step(a)
    return info, env.env.replay_jsonl()


def build_of(replay: str):
    """Endstand: {pos: [typ, tiers]}, erster Turmtyp, Runde des ersten Turms je Typ."""
    towers, first, first_round = {}, None, {}
    for line in replay.splitlines():
        r = json.loads(line)
        for a in r.get("actions", []):
            kind, v = next(iter(a.items()))
            if kind == "place":
                towers[(v["x"], v["y"])] = [v["tower"], [0, 0, 0]]
                first = first or v["tower"]
                first_round.setdefault(v["tower"], r["round"])
            else:
                key = min(towers, key=lambda k: (k[0] - v["x"]) ** 2 + (k[1] - v["y"]) ** 2)
                if kind == "upgrade":
                    towers[key][1][v["path"]] += 1
                else:
                    towers.pop(key)
    return towers, first, first_round


def main_path(tiers):
    return "-" if max(tiers) == 0 else PATHS[int(np.argmax(tiers))]  # argmax: kleinster Index bei Gleichstand


def entropy(counter: Counter) -> float:
    n = sum(counter.values())
    return abs(-sum(c / n * math.log2(c / n) for c in counter.values()))  # abs: kein "-0.00"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("model")
    ap.add_argument("--data", required=True)
    ap.add_argument("--preview", type=int, default=0)
    ap.add_argument("--ban-towers", type=int, default=0)
    ap.add_argument("--episodes", type=int, default=64, help="Spiele ohne Sperre für die Vielfalt")
    ap.add_argument("--per-set", type=int, default=10, help="stochastische Spiele je Sperr-Kombination")
    ap.add_argument("--max-banned", type=int, default=2, help="Kombinationen bis zu so vielen Sperren")
    ap.add_argument("--json", default=None, help="Ergebnisse zusätzlich als JSON speichern")
    ap.add_argument("--recovery", default=None, help="Eval-Satz 'Erholung' (JSON-Liste von Zuständen)")
    ap.add_argument("--skip-bans", action="store_true", help="Teil 2 auslassen")
    ap.add_argument("--lookahead", default=None, help="K,H: Vorausschau vor Rundenstart (docs/design-lookahead.md)")
    args = ap.parse_args()

    from sb3_contrib import MaskablePPO
    model = MaskablePPO.load(args.model, device="cuda")
    env = RustBTDEnv(args.data, record=True, preview_rounds=args.preview, max_banned_towers=args.ban_towers)
    if args.lookahead:
        from btd_sim.lookahead import LookaheadPolicy
        k, h = map(int, args.lookahead.split(","))
        policies = {d: LookaheadPolicy(model, env.env, k, h, deterministic=d) for d in (True, False)}
        global PREDICT
        PREDICT = lambda m, obs, mask, det: policies[det].choose(obs, mask)  # noqa: E731
    names = [t["name"] for t in json.load(open(args.data))["towers"]]
    end = json.load(open(args.data))["rules"]["end_round"]
    none = [False] * len(names)

    # Teil 1: Vielfalt ohne Sperre
    keys, firsts, paths, first_rounds, cells, rounds = Counter(), Counter(), Counter(), {}, Counter(), []
    for _ in range(args.episodes):
        info, replay = play(env, model, none, deterministic=False)
        towers, first, fr = build_of(replay)
        used = sorted({(t, main_path(tiers)) for t, tiers in towers.values()})
        keys[(first, tuple(used))] += 1
        firsts[first] += 1
        paths.update(f"{t}:{p}" for t, p in used)
        for t, r in fr.items():
            first_rounds.setdefault(t, []).append(r)
        cells.update(f"({x:g},{y:g})" for x, y in towers)
        rounds.append(info["round"])
    h = entropy(keys)
    print(f"== {args.model}: Vielfalt ohne Sperre ({args.episodes} Spiele, stochastisch)")
    print(f"H(Schlüssel) = {h:.2f} bit, {len(keys)} verschiedene Schlüssel, Ø Runde {np.mean(rounds):.1f}, "
          f"Siege {sum(r == end for r in rounds)}/{len(rounds)}")
    print("häufigste Schlüssel:", "; ".join(f"{n}× {k[0]} | {' '.join(f'{t}:{p}' for t, p in k[1])}"
                                            for k, n in keys.most_common(5)))
    print("erster Turm:", dict(firsts.most_common()))
    print("Haupt-Pfade (Anteil Spiele):", {k: f"{v / args.episodes:.0%}" for k, v in paths.most_common()})
    print("erster Turm je Typ, Ø Runde:", {t: f"{np.mean(r):.1f} ({len(r)}×)" for t, r in first_rounds.items()})
    print(f"belegte Zellen: {len(cells)} verschiedene, häufigste:", dict(cells.most_common(5)))

    recovery = None
    if args.recovery:
        states = json.load(open(args.recovery))
        det_r, sto_r = [], []
        for s in states:
            for det, acc in ((True, det_r), (False, sto_r)):
                obs = env.env.reset_from(json.dumps(s))
                done, info = False, {}
                while not done:
                    a = PREDICT(model, obs, env.action_masks(), det)
                    obs, _, done, _, info = env.step(a)
                acc.append((info["round"], info["lives"] > 0, info["round"] - s["round"]))
        recovery = {}
        print(f"\n== Erholung ({len(states)} gestörte Zustände, Startrunde Ø {np.mean([s['round'] for s in states]):.1f})")
        for name, acc in (("deterministisch", det_r), ("stochastisch", sto_r)):
            wins = sum(w for _, w, _ in acc)
            gained = float(np.mean([g for _, _, g in acc]))
            recovery[name] = dict(siege=wins, runden_weiter=gained)
            print(f"{name:<16} Siege {wins}/{len(acc)}, Ø {gained:.1f} Runden weitergespielt")

    if args.skip_bans:
        if args.json:
            Path(args.json).write_text(json.dumps(dict(model=args.model, entropy=h, recovery=recovery), indent=1))
        return

    # Teil 2: Robustheit je Sperr-Kombination
    print(f"\n== Überleben je Sperre (det. Runde/Leben | stochastisch Ø Runde, Siege von {args.per_set})")
    table = []
    for k in range(args.max_banned + 1):
        for combo in itertools.combinations(range(len(names)), k):
            banned = [i in combo for i in range(len(names))]
            det, _ = play(env, model, banned, deterministic=True)
            st = [play(env, model, banned, deterministic=False)[0]["round"] for _ in range(args.per_set)]
            label = "keine" if not combo else "ohne " + "+".join(names[i] for i in combo)
            row = dict(sperre=label, det_runde=det["round"], det_leben=det["lives"],
                       sto_runde=float(np.mean(st)), sto_siege=sum(r == end for r in st))
            table.append(row)
            print(f"{label:<26} {det['round']:>3} / {det['lives']:>3} | Ø {np.mean(st):5.1f}  "
                  f"{row['sto_siege']:>2}/{args.per_set}")
    if args.json:
        Path(args.json).write_text(json.dumps(dict(
            model=args.model, entropy=h, keys={f"{k[0]}|{k[1]}": n for k, n in keys.items()},
            first=dict(firsts), paths=dict(paths), rounds=rounds, survival=table, recovery=recovery),
            indent=1))


if __name__ == "__main__":
    main()
