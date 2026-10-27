"""Spiel-Dump -> game_v1 (docs/m2-plan.md, M2b).

Liest den lokalen BTD6-Dump (mod/, data/raw/game_dump/<version>/) und schreibt eine
Spielkonfiguration im Format `game_v1` nach data/local/ (gitignored: abgeleitete Spieldaten
bleiben lokal, das Repo ist öffentlich). Dazu ein Abdeckungs-Report, welche
Upgrade-Kombinationen die Engine schon simulieren kann.

    .venv/bin/python -I tools/build_game_v1.py                       # Monkey Meadow, Medium, Runde 1-40
    .venv/bin/python -I tools/build_game_v1.py --preset chimps        # CHIMPS (Runde 6-100)

`-I`: die Dump-Daten sind fremde Daten; der Konverter liest nur JSON.
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DUMPS = ROOT / "data" / "raw" / "game_dump"
OUT = ROOT / "data" / "local"

# Türme, die der Konverter übersetzt (M2b: gerade Projektile, Fächer, Explosionen)
TOWERS = ["DartMonkey", "TackShooter", "BombShooter"]
TOWER_NAMES = {"DartMonkey": "dart", "TackShooter": "tack", "BombShooter": "bomb"}

# Flächentypen im Spiel (AreaType) -> game_v1
AREA_KINDS = {2: "land", 0: "track", 1: "water", 3: "blocked"}
# BloonProperties-Bits
PROPS = {1: "lead", 2: "black", 4: "white", 8: "purple", 16: "frozen"}

# Model-Typen ohne Einfluss auf die Simulation (Grafik, Sound, UI, Targeting-Optionen)
COSMETIC = ("Display", "Sound", "Effect", "Sprite", "Prefab", "Audio", "Animation", "Eject", "Icon",
            "UpgradePath", "ApplyMod", "TargetType", "RotateToTarget", "Target")
# Model-Typen, die der Konverter auswertet (alles andere gameplay-relevante -> nicht unterstützt)
HANDLED = {"TowerModel", "CircleFootprintModel", "AttackModel", "WeaponModel", "SingleEmissionModel",
           "ArcEmissionModel", "ProjectileModel", "DamageModel", "TravelStraitModel", "ProjectileFilterModel",
           "AttackFilterModel", "FilterInvisibleModel", "CreateProjectileOnContactModel", "AgeModel",
           # zusätzlicher Pierce-Abzug gegen Bloons mit bestimmtem Tag; leerer Tag = keine Wirkung
           # (wird in convert_projectile geprüft)
           "CollideExtraPierceReductionModel"}

PRESETS = {
    # Medium-Standard; Benchmark (a): Runde 1-40
    "medium": dict(start_cash=650, start_lives=150, start_round=1, end_round=40, selling=True,
                   cost_multiplier=1.0),
    # CHIMPS (Werte aus dem Dump: matches/Standard_Clicks_Hard_Tutorial.json)
    "chimps": dict(start_cash=650, start_lives=1, start_round=6, end_round=100, selling=False,
                   cost_multiplier=1.08),
}


def short(t: str) -> str:
    return t.split(",")[0].split(".")[-1]


def load(path: Path):
    return json.loads(path.read_text())


def latest_dump() -> Path:
    versions = sorted((p for p in DUMPS.iterdir() if p.is_dir() and ".tmp" not in p.name),
                      key=lambda p: [int(x) if x.isdigit() else x for x in p.name.split(".")])
    if not versions:
        sys.exit(f"kein Dump unter {DUMPS} (mod/README.md)")
    return versions[-1]


# ---------------------------------------------------------------- Karte

def polygon(points) -> list:
    return [[p["x"], p["y"]] for p in points]


def circle_polygon(x: float, y: float, r: float, n: int = 12) -> list:
    return [[x + r * math.cos(2 * math.pi * k / n), y + r * math.sin(2 * math.pi * k / n)] for k in range(n)]


def contains(poly, x, y) -> bool:
    inside, j = False, len(poly) - 1
    for i in range(len(poly)):
        (xi, yi), (xj, yj) = poly[i], poly[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            inside = not inside
        j = i
    return inside


def dist_to_edges(poly, x, y) -> float:
    best, j = math.inf, len(poly) - 1
    for i in range(len(poly)):
        (ax, ay), (bx, by) = poly[j], poly[i]
        dx, dy = bx - ax, by - ay
        l2 = dx * dx + dy * dy
        t = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / l2)) if l2 > 0 else 0.0
        best = min(best, math.hypot(ax + t * dx - x, ay + t * dy - y))
        j = i
    return best


def placeable(areas, bounds, x, y, r) -> bool:
    """Dieselbe Regel wie btd_core::map::placeable (Flächen)."""
    x0, y0, x1, y1 = bounds
    if x - r < x0 or y - r < y0 or x + r > x1 or y + r > y1:
        return False
    deciding = next((a for a in reversed(areas) if contains(a["polygon"], x, y)), None)
    if deciding is None or deciding["kind"] != "land":
        return False
    return all(not contains(a["polygon"], x, y) and dist_to_edges(a["polygon"], x, y) >= r
               for a in areas if a["kind"] != "land")


def build_map(dump: Path, map_name: str, footprint: float) -> dict:
    mm = load(dump / "maps" / f"{map_name}.json")["map"]
    pts = [[p["point"]["x"], p["point"]["y"]] for p in mm["paths"][0]["points"]]
    cum = [0.0]
    for (ax, ay), (bx, by) in zip(pts, pts[1:]):
        cum.append(cum[-1] + math.hypot(bx - ax, by - ay))

    areas = []
    for a in mm["areas"]:
        kind = AREA_KINDS.get(a["type"])
        if kind is None:
            sys.exit(f"unbekannter Flächentyp {a['type']} ({a.get('name')})")
        areas.append({"kind": kind, "polygon": polygon(a["polygon"]["points"])})
    if areas[0]["kind"] != "land":
        sys.exit("erste Fläche ist nicht das Spielfeld")
    for b in mm.get("blockers") or []:
        if not b.get("isDisabled"):
            c = b["circle"]
            areas.append({"kind": "blocked", "polygon": circle_polygon(c["position"]["x"], c["position"]["y"], c["radius"])})
    xs = [p[0] for p in areas[0]["polygon"]]
    ys = [p[1] for p in areas[0]["polygon"]]
    bounds = [min(xs), min(ys), max(xs), max(ys)]

    # Raster für die Env: 10 Einheiten pro Zelle
    cell = 10.0
    w, h = math.ceil((bounds[2] - bounds[0]) / cell), math.ceil((bounds[3] - bounds[1]) / cell)
    path_rows, build_rows = [], []
    track = [a["polygon"] for a in areas if a["kind"] == "track"]
    for cy in range(h):
        prow, brow = "", ""
        for cx in range(w):
            x, y = bounds[0] + (cx + 0.5) * cell, bounds[1] + (cy + 0.5) * cell
            prow += "#" if any(contains(t, x, y) for t in track) else "."
            brow += "#" if placeable(areas, bounds, x, y, footprint) else "."
        path_rows.append(prow)
        build_rows.append(brow)
    return {
        "name": map_name,
        "bounds": bounds,
        "points": pts,
        "cum": cum,
        "grid": {"x0": bounds[0], "y0": bounds[1], "cell": cell, "w": w, "h": h,
                 "path": path_rows, "buildable": build_rows},
        "placement": "areas",
        "areas": areas,
    }


# ---------------------------------------------------------------- Bloons, Runden, Einkommen

def props(bits: int) -> list:
    return [name for bit, name in PROPS.items() if bits & bit]


def children_counts(children) -> list:
    out = []
    for c in children or []:
        if out and out[-1][0] == c:
            out[-1][1] += 1
        else:
            out.append([c, 1])
    return out


def build_bloons(dump: Path, needed: set) -> list:
    summary = {b["name"]: b for b in load(dump / "summary.json")["bloons"]}
    todo, seen = list(needed), set()
    while todo:
        n = todo.pop()
        if n in seen:
            continue
        seen.add(n)
        todo.extend(summary[n].get("children") or [])
    leak_memo: dict[str, float] = {}

    def leak(n: str) -> float:
        """Lebensverlust beim Durchkommen: eigene Schicht + alle Kinder (= RBE, MOAB 616)."""
        if n not in leak_memo:
            s = summary[n]
            leak_memo[n] = s["leakDamage"] + sum(leak(c) for c in s.get("children") or [])
        return leak_memo[n]

    out = []
    for n in sorted(seen):
        s = summary[n]
        model = load(dump / "bloons" / s["baseId"] / f"{n}.json")
        out.append({
            "name": n, "hp": float(s["maxHealth"]), "speed": float(s["speed"]),
            "leak": int(round(leak(n))),
            "children": children_counts(s.get("children")),
            "props": props(s["bloonProperties"]),
            "classes": (["moab"] if s["isMoab"] else []) + (["ceramic"] if s["baseId"] == "Ceramic" else [])
                       + (["boss"] if s["isBoss"] else []),
            "camo": bool(s["isCamo"]), "regrow": bool(s["isGrow"]), "radius": float(model["radius"]),
        })
    return out


def build_rounds(dump: Path, round_set: str, last: int):
    rounds = []
    for r in range(1, last + 1):
        model = load(dump / "rounds" / round_set / f"{r:03d}.json")
        rounds.append([{"bloon": g["bloon"], "count": g["count"], "start": g["start"] / 60.0, "end": g["end"] / 60.0}
                       for g in model["groups"] if g["count"] > 0])
    return rounds


def build_income(dump: Path, round_set: str, last: int) -> list:
    linked = load(dump / "rounds" / round_set / "_set.json")["linkedIncomeSet"]
    inc = next(s for s in load(dump / "gamedata" / "incomeSets.json") if s["name"] == linked)
    out = []
    for r in range(1, last + 1):
        mult = next((t["multiplier"] for t in inc["thresholds"] if r <= t["threshold"]), inc["finalMultiplier"])
        out.append({"cash_per_pop": float(mult), "end_bonus": float(100 + r)})  # Rundenbonus 100 + r [KAL]
    return out


# ---------------------------------------------------------------- Türme

class Unsupported(Exception):
    pass


def check_types(node, found: set):
    """Sammelt gameplay-relevante Model-Typen, die der Konverter nicht kennt."""
    if isinstance(node, dict):
        t = short(node.get("$type", ""))
        if t and not any(c in t for c in COSMETIC):
            if t not in HANDLED and not t.startswith(("Vector", "Il2Cpp")) and t.endswith("Model"):
                found.add(t)
            for k, v in node.items():
                if not k[:1].isupper():
                    check_types(v, found)
    elif isinstance(node, list):
        for v in node:
            check_types(v, found)


def behaviors(node, kind: str) -> list:
    return [b for b in node.get("behaviors") or [] if short(b.get("$type", "")) == kind]


def camo_filtered(filters) -> bool:
    """True, wenn ein aktiver FilterInvisibleModel Camo-Bloons ausschließt."""
    return any(short(f.get("$type", "")) == "FilterInvisibleModel" and f.get("isActive") for f in filters or [])


def cannot_damage(proj) -> list:
    dmg = behaviors(proj, "DamageModel")
    return props(dmg[0]["immuneBloonProperties"]) if dmg else []


def convert_emission(em, fixed: bool) -> dict:
    et = short(em["$type"])
    if et == "SingleEmissionModel":
        return {"kind": "single"}
    if et == "ArcEmissionModel":
        return {"kind": "arc", "count": em["count"], "angle": em["angle"], "offset": em.get("offset", 0.0),
                "fixed": fixed}
    raise Unsupported(f"Emission {et}")


def check_pierce_reduction(proj):
    for b in behaviors(proj, "CollideExtraPierceReductionModel"):
        if b.get("bloonTag"):
            raise Unsupported(f"CollideExtraPierceReductionModel(bloonTag={b['bloonTag']})")


def convert_contacts(proj) -> list:
    """CreateProjectileOnContactModel -> Explosion (ohne Flugbahn) oder Splitter (mit Flugbahn)."""
    out = []
    for c in behaviors(proj, "CreateProjectileOnContactModel"):
        sub = c["projectile"]
        check_pierce_reduction(sub)
        if behaviors(sub, "CreateProjectileOnContactModel"):
            raise Unsupported("verschachtelte Kontakt-Projektile")
        sub_dmg = behaviors(sub, "DamageModel")
        common = {"pierce": int(sub["pierce"]), "damage": sub_dmg[0]["damage"] if sub_dmg else 0.0,
                  "cannot_damage": cannot_damage(sub), "camo": not camo_filtered(sub.get("filters"))}
        travel = behaviors(sub, "TravelStraitModel")
        if travel:
            out.append({"kind": "shards", "emission": convert_emission(c["emission"], False),
                        "speed": travel[0]["speed"], "lifespan": travel[0]["lifespan"], "radius": sub["radius"],
                        **common})
        else:
            if short(c["emission"]["$type"]) != "SingleEmissionModel":
                raise Unsupported("Explosion mit Mehrfach-Emission")
            out.append({"kind": "explosion", "radius": sub["radius"], **common})
    return out


def convert_attack(tower, attack, weapon, idx) -> dict:
    proj = weapon["projectile"]
    check_pierce_reduction(proj)
    dmg = behaviors(proj, "DamageModel")
    travel = behaviors(proj, "TravelStraitModel")
    if not travel:
        raise Unsupported("Projektil ohne TravelStraitModel")
    emission = convert_emission(weapon["emission"], bool(tower.get("doesntRotate")))
    on_contact = convert_contacts(proj)
    attack_camo = not (camo_filtered(proj.get("filters"))
                       or any(camo_filtered(b.get("filters")) for b in behaviors(attack, "AttackFilterModel")))
    return {
        "tag": f"{attack.get('name', 'attack')}-{idx}".replace(" ", "_"),
        "range": attack["range"] if attack["range"] != tower["range"] else None,
        "cooldown": weapon["rate"],
        "damage": dmg[0]["damage"] if dmg else 0.0,
        "pierce": int(proj["pierce"]),
        "cannot_damage": cannot_damage(proj),
        "camo": attack_camo,
        "delivery": {"kind": "projectile", "speed": travel[0]["speed"], "lifespan": travel[0]["lifespan"],
                     "radius": proj["radius"], "emission": emission, "on_contact": on_contact},
    }


def convert_variant(model, tiers) -> dict:
    unknown = set()
    check_types(model, unknown)
    if unknown:
        raise Unsupported(", ".join(sorted(unknown)))
    attacks = []
    for a in behaviors(model, "AttackModel"):
        for i, w in enumerate(a.get("weapons") or []):
            attacks.append(convert_attack(model, a, w, len(attacks)))
    if not attacks:
        raise Unsupported("kein Angriff")
    for a in attacks:
        if a["range"] is None:
            del a["range"]
    return {"tiers": tiers, "range": model["range"], "footprint": model["footprint"]["radius"], "attacks": attacks}


def build_tower(dump: Path, base: str, coverage: list) -> dict:
    tdir = dump / "towers" / base
    base_model = load(tdir / f"{base}.json")
    variants, costs = [], [[], [], []]
    # Upgrade-Kosten aus den Einzelpfad-Modellen: neues Upgrade = letzter Eintrag in appliedUpgrades
    for p in range(3):
        for k in range(1, 6):
            tiers = [0, 0, 0]
            tiers[p] = k
            m = load(tdir / f"{base}-{''.join(map(str, tiers))}.json")
            name = m["appliedUpgrades"][-1]
            costs[p].append(int(load(dump / "upgrades" / f"{name}.json")["cost"]))
    for f in sorted(tdir.glob(f"{base}-[0-5][0-5][0-5].json")) + [tdir / f"{base}.json"]:
        code = f.stem.split("-")[-1] if "-" in f.stem else "000"
        tiers = [int(c) for c in code]
        try:
            variants.append(convert_variant(load(f), tiers))
            coverage.append((base, code, None))
        except Unsupported as e:
            coverage.append((base, code, str(e)))
    if not any(v["tiers"] == [0, 0, 0] for v in variants):
        sys.exit(f"{base}: Basisvariante wird nicht unterstützt: {next(c[2] for c in coverage if c[:2] == (base, '000'))}")
    variants.sort(key=lambda v: v["tiers"])
    return {"name": TOWER_NAMES[base], "cost": int(base_model["cost"]), "upgrade_costs": costs, "variants": variants}


# ---------------------------------------------------------------- Main

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dump", type=Path, default=None)
    ap.add_argument("--map", default="Tutorial", help="interner Kartenname (Monkey Meadow = Tutorial)")
    ap.add_argument("--preset", choices=sorted(PRESETS), default="medium")
    ap.add_argument("--rounds", default="DefaultRoundSet")
    args = ap.parse_args()
    dump = args.dump or latest_dump()
    preset = PRESETS[args.preset]

    coverage = []
    towers = [build_tower(dump, t, coverage) for t in TOWERS]
    footprint = max(v["footprint"] for t in towers for v in t["variants"])
    last = preset["end_round"]
    rounds = build_rounds(dump, args.rounds, last)
    needed = {g["bloon"] for r in rounds for g in r}
    data = {
        "format": "game_v1",
        "name": f"{args.map}-{args.preset}-{dump.name}",
        "rules": {"tick_rate": 60.0, "cd_eps": 1e-9, "max_round_seconds": 900.0, "child_spacing": 0.0,
                  "sell_ratio": 0.7, "cost_rounding": 5, **preset},
        "map": build_map(dump, args.map, footprint),
        "bloons": build_bloons(dump, needed),
        "towers": towers,
        "rounds": rounds,
        "income": build_income(dump, args.rounds, last),
    }
    OUT.mkdir(parents=True, exist_ok=True)
    stem = f"game_v1_{args.map.lower()}_{args.preset}"
    (OUT / f"{stem}.json").write_text(json.dumps(data, indent=1) + "\n")

    lines = [f"# Abdeckung {stem} (Dump {dump.name})", "",
             "Erzeugt von tools/build_game_v1.py. Nicht unterstützte Kombinationen sind im Aktionsraum gesperrt.", ""]
    for base in TOWERS:
        rows = [c for c in coverage if c[0] == base]
        ok = sum(1 for c in rows if c[2] is None)
        lines += [f"## {base}: {ok}/{len(rows)} Kombinationen", ""]
        missing: dict[str, list] = {}
        for _, code, why in rows:
            if why:
                missing.setdefault(why, []).append(code)
        for why, codes in sorted(missing.items(), key=lambda x: -len(x[1])):
            lines.append(f"- {len(codes)}× fehlt `{why}`: {', '.join(sorted(codes))}")
        lines.append("")
    (OUT / f"{stem}_coverage.md").write_text("\n".join(lines))
    total = sum(1 for c in coverage if c[2] is None)
    print(f"data/local/{stem}.json: {len(data['bloons'])} Bloons, {len(rounds)} Runden, "
          f"{total}/{len(coverage)} Turm-Kombinationen unterstützt (Report: data/local/{stem}_coverage.md)")


if __name__ == "__main__":
    main()
