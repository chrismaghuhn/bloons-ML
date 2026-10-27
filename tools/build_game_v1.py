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

# Die 6 klassischen Primary-Türme (docs/m2-plan.md)
TOWERS = ["DartMonkey", "BoomerangMonkey", "BombShooter", "TackShooter", "IceMonkey", "GlueGunner"]
TOWER_NAMES = {"DartMonkey": "dart", "BoomerangMonkey": "boomerang", "BombShooter": "bomb",
               "TackShooter": "tack", "IceMonkey": "ice", "GlueGunner": "glue"}

# Flächentypen im Spiel (AreaType) -> game_v1
AREA_KINDS = {2: "land", 0: "track", 1: "water", 3: "blocked"}
# BloonProperties-Bits
PROPS = {1: "lead", 2: "black", 4: "white", 8: "purple", 16: "frozen"}

# Model-Typen ohne Einfluss auf die Simulation: Grafik/Sound (Teilstring) und eine feste Liste
# (Targeting-Optionen: die Sim zielt immer "First"; Upgrade-/Mod-Verweise)
COSMETIC_PARTS = ("Display", "Sound", "Effect", "Sprite", "Prefab", "Audio", "Animation", "Icon")
COSMETIC_TYPES = {"UpgradePathModel", "ApplyModModel", "TargetType", "RotateToTargetModel",
                  "TargetFirstModel", "TargetLastModel", "TargetCloseModel", "TargetStrongModel",
                  "TargetFirstPrioCamoModel", "TargetLastPrioCamoModel", "TargetClosePrioCamoModel",
                  "TargetStrongPrioCamoModel"}


def cosmetic(t: str) -> bool:
    return t in COSMETIC_TYPES or any(c in t for c in COSMETIC_PARTS)
# Model-Typen, die der Konverter auswertet (alles andere gameplay-relevante -> nicht unterstützt)
HANDLED = {"TowerModel", "CircleFootprintModel", "AttackModel", "WeaponModel", "SingleEmissionModel",
           "ArcEmissionModel", "ProjectileModel", "DamageModel", "TravelStraitModel", "ProjectileFilterModel",
           "AttackFilterModel", "CreateProjectileOnContactModel", "AgeModel",
           # zusätzlicher Pierce-Abzug gegen Bloons mit bestimmtem Tag; leerer Tag = keine Wirkung
           # (wird in check_pierce_reduction geprüft)
           "CollideExtraPierceReductionModel",
           # Filter (convert_filters)
           "FilterInvisibleModel", "FilterOutTagModel", "FilterBloonIfDamageTypeModel", "FilterFrozenBloonsModel",
           "FilterGlueLevelModel", "FilterOveridingMutatedTargetModel", "FilterMoabModel", "FilterWithTagModel",
           "FilterOutBloonModel",
           # Bonus-Schaden und Status-Effekte (convert_extras)
           "DamageModifierForTagModel", "FreezeModel", "FreezeModifierForTagsModel", "SlowModel",
           "SlowModifierForTagModel",
           # Kurvenbahn (Boomerang) und Flächentreffer mit Turm-Reichweite (Ice)
           "FollowPathModel", "LinkProjectileRadiusToTowerRangeModel",
           # ohne Wirkung in der Sim: Regrow ist noch nicht simuliert (GrowBlock verhindert es nur),
           # Drehung/Spiegelung nur Darstellung [KAL: FlipFollowPath spiegelt evtl. die Kurve],
           # Wasser einfrieren gibt es auf Monkey Meadow nicht
           "GrowBlockModel", "RotateModel", "FlipFollowPathModel", "FreezeNearbyWaterModel"}

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
            "tags": list(s.get("tags") or []),
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
    """Sammelt gameplay-relevante Model-Typen, die der Konverter nicht kennt (auch unterhalb
    kosmetischer Knoten, damit dort nichts Spielrelevantes durchrutscht)."""
    if isinstance(node, dict):
        t = short(node.get("$type", ""))
        if t and not cosmetic(t) and t not in HANDLED and not t.startswith(("Vector", "Il2Cpp")) \
                and t.endswith("Model"):
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
        if em.get("offsetStart", 0.0) or em.get("sliceSize", 0.0) or em.get("useAirUnitRotation"):
            raise Unsupported("ArcEmissionModel(offsetStart/sliceSize/AirUnit)")
        return {"kind": "arc", "count": em["count"], "angle": em["angle"], "offset": em.get("offset", 0.0),
                "fixed": fixed}
    raise Unsupported(f"Emission {et}")


def rotates(tower) -> bool:
    """Ob der Turm auf sein Ziel ausgerichtet schießt (Spiel: RotateToTargetModel am Angriff).
    Tack hat keins: seine Nadeln fliegen in feste Richtungen."""
    found = set()

    def walk(n):
        if isinstance(n, dict):
            if short(n.get("$type", "")) == "RotateToTargetModel":
                found.add(1)
            for k, v in n.items():
                if not k[:1].isupper():
                    walk(v)
        elif isinstance(n, list):
            for v in n:
                walk(v)
    walk(tower)
    return bool(found)


def check_damage(proj):
    for d in behaviors(proj, "DamageModel"):
        if d.get("maxDamage", 0.0) > 0.0:
            raise Unsupported("DamageModel(maxDamage)")


def check_pierce_reduction(proj):
    for b in behaviors(proj, "CollideExtraPierceReductionModel"):
        if b.get("bloonTag"):
            raise Unsupported(f"CollideExtraPierceReductionModel(bloonTag={b['bloonTag']})")


def all_filters(*nodes) -> list:
    """Filter eines Projektils bzw. Angriffs inkl. ProjectileFilterModel/AttackFilterModel."""
    out = []
    for n in nodes:
        out += n.get("filters") or []
        for b in behaviors(n, "ProjectileFilterModel") + behaviors(n, "AttackFilterModel"):
            out += b.get("filters") or []
    return out


def convert_filters(filters) -> tuple:
    """-> (camo, filter-Dict für game_v1)."""
    camo, f = True, {}
    for x in filters:
        t = short(x.get("$type", ""))
        if t == "FilterInvisibleModel":
            if x.get("isActive"):
                camo = False
        elif t == "FilterOutTagModel":
            f.setdefault("exclude_tags", [])
            if x["tag"] not in f["exclude_tags"]:
                f["exclude_tags"].append(x["tag"])
        elif t == "FilterBloonIfDamageTypeModel":
            f["skip_immune"] = True
        elif t == "FilterFrozenBloonsModel":
            f["skip_frozen"] = True
        elif t == "FilterGlueLevelModel":
            f["skip_glue_level"] = max(f.get("skip_glue_level", 0), int(x["glueLevel"]))
        elif t == "FilterOveridingMutatedTargetModel":
            pass  # Zielreihenfolge zwischen "brüchigen" Bloons (Ice-Upgrades); ohne die Mechanik wirkungslos [KAL]
        elif t in ("FilterMoabModel", "FilterWithTagModel", "FilterOutBloonModel"):
            if t == "FilterMoabModel" and x.get("flip"):
                tag = "Moabs"
            elif t == "FilterWithTagModel" and not x.get("inclusive"):
                tag = x["tag"]
            elif t == "FilterOutBloonModel":
                tag = x["bloonId"]
            else:
                raise Unsupported(f"Filter {t} (nur bestimmte Bloons)")
            f.setdefault("exclude_tags", [])
            if tag not in f["exclude_tags"]:
                f["exclude_tags"].append(tag)
        else:
            raise Unsupported(f"Filter {t}")
    return camo, f


def convert_extras(proj, filters) -> tuple:
    """-> (camo, extras-Dict): Filter, Bonus-Schaden, Freeze, Slow."""
    camo, filt = convert_filters(filters)
    ex = {}
    if filt:
        ex["filter"] = filt
    bonus = []
    for b in behaviors(proj, "DamageModifierForTagModel"):
        if b.get("damageMultiplier", 1.0) != 1.0 or b.get("mustIncludeAllTags") or b.get("ignoreTag"):
            raise Unsupported("DamageModifierForTagModel(Multiplikator/Tag-Logik)")
        bonus.append({"tag": b["tag"], "add": b["damageAddative"]})
    if bonus:
        ex["damage_vs"] = bonus
    freeze = behaviors(proj, "FreezeModel")
    if freeze:
        fz = freeze[0]
        if fz.get("enablePercentChanceToFreeze") or fz.get("canFreezeMoabs") or fz.get("speed", 0.0) != 0.0:
            raise Unsupported("FreezeModel(Chance/MOABs/Teil-Verlangsamung)")
        scale = [{"tag": t, "factor": m["freezeTimeMultiplier"]}
                 for m in behaviors(proj, "FreezeModifierForTagsModel") for t in m["tags"]]
        ex["freeze"] = {"seconds": fz["lifespan"], "layers": int(fz["layers"]), "scale": scale}
    slow = behaviors(proj, "SlowModel")
    if slow:
        sl = slow[0]
        if sl.get("chance", 0.0) not in (0.0, 1.0):
            raise Unsupported("SlowModel(Zufall)")
        over = [{"tag": m["tag"], "factor": m["slowMultiplier"]} for m in behaviors(proj, "SlowModifierForTagModel")
                if not m.get("makeNotTag")]
        ex["slow"] = {"multiplier": sl["multiplier"], "seconds": sl["lifespan"], "level": int(sl.get("glueLevel", 0)),
                      "layers": int(sl["layers"]), "multiplier_for": over}
    elif behaviors(proj, "SlowModifierForTagModel"):
        raise Unsupported("SlowModifierForTagModel ohne SlowModel")
    return camo, ex


def convert_contacts(proj) -> list:
    """CreateProjectileOnContactModel -> Explosion (ohne Flugbahn) oder Splitter (mit Flugbahn)."""
    out = []
    for c in behaviors(proj, "CreateProjectileOnContactModel"):
        if c.get("dontCreateAtBloon") or c.get("useChance") or c.get("passOnDirectionToContact"):
            raise Unsupported("CreateProjectileOnContactModel(dontCreateAtBloon/Chance/Richtung)")
        sub = c["projectile"]
        check_pierce_reduction(sub)
        check_damage(sub)
        if behaviors(sub, "CreateProjectileOnContactModel"):
            raise Unsupported("verschachtelte Kontakt-Projektile")
        sub_dmg = behaviors(sub, "DamageModel")
        sub_camo, sub_ex = convert_extras(sub, all_filters(sub))
        common = {"pierce": int(sub["pierce"]), "damage": sub_dmg[0]["damage"] if sub_dmg else 0.0,
                  "cannot_damage": cannot_damage(sub), "camo": sub_camo, "extras": sub_ex}
        travel = behaviors(sub, "TravelStraitModel")
        if travel:
            em = c["emission"]
            # ohne useProjectileRotation zeigen die Splitter-Richtungen nicht mit der Flugrichtung mit
            out.append({"kind": "shards", "emission": convert_emission(em, not em.get("useProjectileRotation", False)),
                        "speed": travel[0]["speed"], "lifespan": travel[0]["lifespan"], "radius": sub["radius"],
                        "inherit_hit": bool(c.get("passOnCollidedWith")), **common})
        else:
            if short(c["emission"]["$type"]) != "SingleEmissionModel":
                raise Unsupported("Explosion mit Mehrfach-Emission")
            out.append({"kind": "explosion", "radius": sub["radius"], **common})
    return out


def convert_attack(tower, attack, weapon, idx) -> dict:
    proj = weapon["projectile"]
    check_pierce_reduction(proj)
    check_damage(proj)
    dmg = behaviors(proj, "DamageModel")
    travel = behaviors(proj, "TravelStraitModel")
    follow = behaviors(proj, "FollowPathModel")
    age = behaviors(proj, "AgeModel")
    camo, extras = convert_extras(proj, all_filters(proj, attack))
    if travel and age:
        raise Unsupported("AgeModel an einem geraden Projektil")
    if travel or follow:
        emission = convert_emission(weapon["emission"], not rotates(tower))
        if follow:
            fp = follow[0]
            path = [[v["x"], v["y"]] for v in fp["path"]]
            lifespan = age[0]["lifespan"] if age else 60.0
            delivery = {"kind": "projectile", "speed": fp["speed"], "lifespan": lifespan, "radius": proj["radius"],
                        "emission": emission, "path": path, "on_contact": convert_contacts(proj)}
        else:
            delivery = {"kind": "projectile", "speed": travel[0]["speed"], "lifespan": travel[0]["lifespan"],
                        "radius": proj["radius"], "emission": emission, "on_contact": convert_contacts(proj)}
    else:
        # kein Flug: Flächentreffer um den Turm (Ice), Radius = Reichweite
        if behaviors(proj, "CreateProjectileOnContactModel"):
            raise Unsupported("Flächentreffer mit Kontakt-Projektilen")
        if short(weapon["emission"]["$type"]) != "SingleEmissionModel":
            raise Unsupported("Flächentreffer mit Mehrfach-Emission")
        delivery = {"kind": "area"}
    return {
        "tag": f"{attack.get('name', 'attack')}-{idx}".replace(" ", "_"),
        "range": attack["range"] if attack["range"] != tower["range"] else None,
        "cooldown": weapon["rate"],
        "damage": dmg[0]["damage"] if dmg else 0.0,
        "pierce": int(proj["pierce"]),
        "cannot_damage": cannot_damage(proj),
        "camo": camo,
        "delivery": delivery,
        "extras": extras,
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
             "Erzeugt von tools/build_game_v1.py. Nicht unterstützte Kombinationen sind im Aktionsraum gesperrt.", "",
             "Bekannte Vereinfachungen der Sim (gelten für alle Kombinationen):",
             "- Keine Sichtlinie: Bäume/Blocker halten im Spiel Schüsse auf, in der Sim nicht.",
             "- Targeting immer \"First\"; Explosionen wirken sofort statt 0,1 s.",
             "- Fähigkeiten (AbilityModel) sind bewusst nicht simuliert (Design: M4); "
             "Kombinationen damit bleiben gesperrt.", ""]
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
