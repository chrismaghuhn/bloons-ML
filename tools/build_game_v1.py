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
# Turm-Sets (--towers): primary = die 6 klassischen Primary-Türme, t12 = dazu Schritt 1 aus
# docs/design-towers-step1.md
TOWER_SETS = {"primary": TOWERS,
              "t12": TOWERS + ["SniperMonkey", "Desperado", "SuperMonkey", "Druid", "NinjaMonkey", "WizardMonkey"]}
TOWER_NAMES = {"DartMonkey": "dart", "BoomerangMonkey": "boomerang", "BombShooter": "bomb",
               "TackShooter": "tack", "IceMonkey": "ice", "GlueGunner": "glue", "SniperMonkey": "sniper",
               "Desperado": "desperado", "SuperMonkey": "super", "Druid": "druid", "NinjaMonkey": "ninja",
               "WizardMonkey": "wizard"}

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
                  "TargetStrongPrioCamoModel", "ShowTextOnHitModel",
                  # Desperado-Targeting (die Sim zielt immer "First"); Super-Monkey-Radius ist Darstellung
                  "TargetDesperadoFirstModel", "TargetDesperadoLastModel", "TargetDesperadoCloseModel",
                  "TargetDesperadoStrongModel", "TowerRadiusModel"}


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
           # Bonus-Schaden und Status-Effekte (convert_extras)
           "DamageModifierForTagModel", "FreezeModel", "FreezeModifierForTagsModel", "SlowModel",
           "SlowModifierForTagModel",
           # Kurvenbahn (Boomerang) und Flächentreffer mit Turm-Reichweite (Ice)
           "FollowPathModel", "LinkProjectileRadiusToTowerRangeModel",
           # ohne Wirkung in der Sim: Regrow ist noch nicht simuliert (GrowBlock verhindert es nur),
           # Drehung/Spiegelung nur Darstellung [KAL: FlipFollowPath spiegelt evtl. die Kurve],
           # Wasser einfrieren gibt es auf Monkey Meadow nicht
           "GrowBlockModel", "RotateModel", "FlipFollowPathModel", "FreezeNearbyWaterModel",
           # M2c: Zufallsreichweite, Verbrauchs-Projektile (Cluster), Schaden über Zeit, Zurückstoßen,
           # Treffer-Reset, kritische Treffer, parallele Emission, Verlangsamungs-Zone
           "RandomRangeTravelStraitModel", "CreateProjectileOnExhaustFractionModel", "AddBehaviorToBloonModel",
           "DamageOverTimeModel", "AddBehaviorModifierForTagModel", "PushBackModel", "ClearHitBloonsModel",
           "CritMultiplierModel", "ParallelEmissionModel", "SlowBloonsZoneModel",
           # Türme Schritt 1: Zielen ohne Abwurf-Versatz (die Sim zielt ohnehin von der Turmmitte)
           "CheckTargetsWithoutOffsetsModel"}

# Nur dort erlaubt, wo der Konverter sie wirklich auswertet (`use`); jedes andere Vorkommen (z.B. in
# einem Kontakt-Projektil eines späteren Upgrades) macht die Variante "nicht unterstützt".
STRICT = {"InstantModel", "InstantDamageEmissionModel", "FilterAllExceptTargetModel", "BurstWeaponBehaviorModel",
          "RandomArcEmissionModel", "RandomEmissionModel", "OffsetModel", "RemoveBloonModifiersModel",
          "TrackTargetModel", "TrackTargetWithinTimeModel"}
HANDLED |= STRICT
_used: set = set()


def use(node):
    _used.add(id(node))


def check_strict(node):
    """Wirft Unsupported, wenn ein STRICT-Modell an einer nicht ausgewerteten Stelle steht."""
    if isinstance(node, dict):
        t = short(node.get("$type", ""))
        if t in STRICT and id(node) not in _used:
            raise Unsupported(f"{t} (an dieser Stelle)")
        for k, v in node.items():
            if not k[:1].isupper():
                check_strict(v)
    elif isinstance(node, list):
        for v in node:
            check_strict(v)

# Bloon-Tags der MOAB-Klassen für PushBackModel-Multiplikatoren
PUSH_TAGS = (("Moab", "multiplierMOAB"), ("Bfb", "multiplierBFB"), ("Ddt", "multiplierDDT"), ("Zomg", "multiplierZOMG"))

PRESETS = {
    # Medium-Standard; Benchmark (a): Runde 1-40
    "medium": dict(start_cash=650, start_lives=150, start_round=1, end_round=40, selling=True,
                   cost_multiplier=1.0, match="Standard_Standard_Medium"),
    # Medium bis zum regulären Ende; Benchmark (b): Runde 1-60
    "medium60": dict(start_cash=650, start_lives=150, start_round=1, end_round=60, selling=True,
                     cost_multiplier=1.0, match="Standard_Standard_Medium"),
    # CHIMPS (Werte aus dem Dump: matches/Standard_Clicks_Hard_Tutorial.json)
    "chimps": dict(start_cash=650, start_lives=1, start_round=6, end_round=100, selling=False,
                   cost_multiplier=1.08, match="Standard_Clicks_Hard"),
}

# Laufstrecke auf dem Pfad: Bloons erscheinen erst bei `start` und leaken bei `end` (Distanz auf der
# Pfad-Polylinie des Dumps). Gemessen mit der Recorder-Mod (docs/design-recorder.md §6, Runde 1 auf
# Monkey Meadow: Spawn bei spawnDist − 10, Leak bei leakDist + ~8,45). Abgeleitete Messwerte, keine
# Rohdaten; sobald die Mod spawnDist/leakDist mitdumpt, kommen sie von dort. [KAL: Leak-Punkt nur für
# Reds gemessen, gilt hier für alle Bloons]
TRACK_SPAN = {"Tutorial": (173.6, 1085.67)}


def bloon_speed_factor(dump: Path, match: str, map_name: str) -> float:
    """Tempo-Faktor der Schwierigkeit (GlobalSpeedModModel in den Partie-Regeln, z.B. Medium 1,1,
    Hard/CHIMPS 1,25); ohne Modell 1,0."""
    path = dump / "matches" / f"{match}_{map_name}.json"
    if not path.exists():
        sys.exit(f"{path} fehlt: einmal eine Partie in diesem Modus mit der Mod starten")
    factors = []

    def walk(n):
        if isinstance(n, dict):
            if short(n.get("$type", "")) == "GlobalSpeedModModel":
                if n.get("addition", 0.0):
                    sys.exit(f"{path}: GlobalSpeedModModel mit addition={n['addition']} nicht unterstützt")
                factors.append(float(n["multiplier"]))
            for v in n.values():
                walk(v)
        elif isinstance(n, list):
            for v in n:
                walk(v)
    walk(load(path).get("mods", []))
    if len(factors) > 1:
        sys.exit(f"{path}: mehrere GlobalSpeedModModel ({factors})")
    return factors[0] if factors else 1.0


def trim_path(pts: list, start: float, end: float) -> list:
    """Teil der Polylinie von Distanz `start` bis `end` (Endpunkte interpoliert)."""
    if not 0.0 <= start < end:
        sys.exit(f"Laufstrecke {start}..{end}: erwartet 0 <= Start < Ende")
    out, d = [], 0.0
    for (ax, ay), (bx, by) in zip(pts, pts[1:]):
        seg = math.hypot(bx - ax, by - ay)
        for target in (start, end):
            if d <= target <= d + seg and seg > 0:
                t = (target - d) / seg
                p = [ax + (bx - ax) * t, ay + (by - ay) * t]
                if target == start:
                    out = [p]
                elif out:
                    out.append(p)
                    return out
        if out and d + seg < end:
            out.append([bx, by])
        d += seg
    sys.exit(f"Laufstrecke {start}..{end} passt nicht zum Pfad (Länge {d:.1f})")


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
    if map_name in TRACK_SPAN:
        pts = trim_path(pts, *TRACK_SPAN[map_name])
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


def build_bloons(dump: Path, needed: set, speed_factor: float = 1.0) -> list:
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
            "name": n, "hp": float(s["maxHealth"]), "speed": float(s["speed"]) * speed_factor,
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
        # negative Startzeiten (CHIMPS Runde 86: -0,15 s) auf 0 klemmen; das Ende bleibt [KAL]
        rounds.append([{"bloon": g["bloon"], "count": g["count"], "start": max(0.0, g["start"] / 60.0),
                        "end": max(0.0, g["end"] / 60.0)}
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
    if et == "RandomArcEmissionModel":
        # zufällige Richtung im Fächer: Mittelwert statt Zufall (Super Monkey: 1 Projektil ±8° -> gerade) [KAL]
        if em.get("offsetStart", 0.0) or em.get("sliceSize", 0.0) or em.get("useAirUnitRotation") \
                or em.get("useProjectileRotation"):
            raise Unsupported("RandomArcEmissionModel(offsetStart/sliceSize/Rotation)")
        use(em)
        if em["count"] == 1 and not em.get("offset", 0.0):
            return {"kind": "single"}
        return {"kind": "arc", "count": em["count"], "angle": em["angle"], "offset": em.get("offset", 0.0),
                "fixed": fixed}
    if et == "RandomEmissionModel":
        # n Projektile mit zufälliger Richtung in einem Winkel: gleichmäßig verteilt (Druid-Dornen);
        # [KAL: angle als voller Fächer angenommen, nicht ±angle; startOffset/ejectPointRandomness ignoriert]
        if em.get("useSpeedMultiplier") or em.get("useMainAttackRotation"):
            raise Unsupported("RandomEmissionModel(SpeedMultiplier/MainAttackRotation)")
        use(em)
        return {"kind": "arc", "count": em["count"], "angle": em["angle"], "offset": 0.0, "fixed": fixed}
    if et == "ParallelEmissionModel":
        n, width = em["count"], em["spreadLength"]
        # Projektile gleichmäßig auf spreadLength quer zur Schussrichtung, mittig; yOffset (Versatz nach
        # vorn, ~1 Einheit) ignoriert [KAL]
        if em.get("linear") or abs(em.get("offsetStart", 0.0) + width / 2) > 1e-9 or n < 1:
            raise Unsupported("ParallelEmissionModel(linear/asymmetrisch)")
        return {"kind": "parallel", "count": n, "spread": width / (n - 1) if n > 1 else 0.0}
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


def pierce_of(proj) -> int:
    """Pierce mit Obergrenze: maxPierce > 0 deckelt ihn (Spiel: CappedPierce, z.B. Glue-Klumpen 4 -> 1)."""
    p = proj["pierce"]
    if proj.get("maxPierce", 0.0) > 0.0:
        p = min(p, proj["maxPierce"])
    return int(p)


def straight_travel(proj):
    """-> (speed, lifespan) einer geraden Flugbahn oder None."""
    fixed, rnd = behaviors(proj, "TravelStraitModel"), behaviors(proj, "RandomRangeTravelStraitModel")
    if len(fixed) + len(rnd) > 1:
        raise Unsupported("mehrere Flugbahnen")
    if fixed:
        return fixed[0]["speed"], fixed[0]["lifespan"]
    if rnd:
        r = rnd[0]
        # Zufallsreichweite (Cluster-Splitter): Mittelwert statt Zufall, die Sim ist deterministisch [KAL]
        return r["speed"], (r["minRange"] + r["maxRange"]) / 2 / r["speed"]
    return None


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
            # eigene Maske (Ice: nur Lead), nicht das cannot_damage des Angriffs (Ice: Lead + White)
            for name in props(x["ifCantHitBloonProperties"]):
                f.setdefault("skip_props", [])
                if name not in f["skip_props"]:
                    f["skip_props"].append(name)
        elif t == "FilterFrozenBloonsModel":
            f["skip_frozen"] = True
        elif t == "FilterGlueLevelModel":
            f["skip_glue_level"] = max(f.get("skip_glue_level", 0), int(x["glueLevel"]))
        elif t == "FilterAllExceptTargetModel":
            pass  # nur bei Sofort-Treffern ausgewertet (convert_attack), sonst über check_strict gesperrt
        elif t == "FilterOveridingMutatedTargetModel":
            pass  # Zielreihenfolge zwischen "brüchigen" Bloons (Ice-Upgrades); ohne die Mechanik wirkungslos [KAL]
        elif t in ("FilterMoabModel", "FilterWithTagModel"):
            if t == "FilterMoabModel" and x.get("flip"):
                tag = "Moabs"
            elif t == "FilterWithTagModel" and not x.get("inclusive"):
                tag = x["tag"]
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
    if len(freeze) > 1:
        raise Unsupported("mehrere FreezeModel")
    if freeze:
        fz = freeze[0]
        if fz.get("enablePercentChanceToFreeze") or fz.get("canFreezeMoabs") or fz.get("speed", 0.0) != 0.0:
            raise Unsupported("FreezeModel(Chance/MOABs/Teil-Verlangsamung)")
        if not fz.get("applyAfterDamage", True):
            raise Unsupported("FreezeModel(vor dem Schaden)")
        own = (fz.get("damageModel") or {}).get("immuneBloonProperties")
        if own is not None and props(own) != cannot_damage(proj):
            raise Unsupported("FreezeModel mit eigener Immunität")
        mods = behaviors(proj, "FreezeModifierForTagsModel")
        if any(m.get("preventMutation") for m in mods):
            raise Unsupported("FreezeModifierForTagsModel(preventMutation)")
        scale = [{"tag": t, "factor": m["freezeTimeMultiplier"]} for m in mods for t in m["tags"]]
        ex["freeze"] = {"seconds": fz["lifespan"], "layers": int(fz["layers"]), "scale": scale}
    slow = behaviors(proj, "SlowModel")
    if len(slow) > 1:
        raise Unsupported("mehrere SlowModel")
    if slow:
        sl = slow[0]
        if sl.get("chance", 0.0) not in (0.0, 1.0):
            raise Unsupported("SlowModel(Zufall)")
        # cascadeMutators (Permafrost/Freeze true, Glue false) bleibt unausgewertet: Kinder erben in
        # der Sim über `layers` [KAL]
        if sl.get("dontRefreshDuration") or sl.get("matchLayersWithDamage") or sl.get("removeMutatorIfNotMatching"):
            raise Unsupported("SlowModel(dontRefresh/matchLayers)")
        over, secs = [], []
        for m in behaviors(proj, "SlowModifierForTagModel"):
            if m.get("makeNotTag") or m.get("slowId", sl.get("mutationId")) != sl.get("mutationId"):
                raise Unsupported("SlowModifierForTagModel(makeNotTag/fremder Slow)")
            if m.get("resetToUnmodified") or m.get("preventMutation"):
                over.append({"tag": m["tag"], "factor": 1.0})  # Bloons mit dem Tag werden nicht verlangsamt
                continue
            # sonst skaliert slowMultiplier den Faktor (MOAB Glue: 0.25 * 2.5 = 0.625)
            f = sl["multiplier"] * m["slowMultiplier"]
            if f > 1.0:
                raise Unsupported("SlowModifierForTagModel(beschleunigt)")
            over.append({"tag": m["tag"], "factor": f})
            if m.get("lifespanOverride", 0.0) > 0.0:
                secs.append({"tag": m["tag"], "factor": m["lifespanOverride"]})
        ex["slow"] = {"id": sl.get("mutationId") or "", "multiplier": sl["multiplier"], "seconds": sl["lifespan"], "level": int(sl.get("glueLevel", 0)),
                      "layers": int(sl["layers"]), "multiplier_for": over}
        if secs:
            ex["slow"]["seconds_for"] = secs
    elif behaviors(proj, "SlowModifierForTagModel"):
        raise Unsupported("SlowModifierForTagModel ohne SlowModel")
    dot = convert_dot(proj, slow[0].get("mutationId") if slow else None)
    if dot:
        ex["dot"] = dot
    push = behaviors(proj, "PushBackModel")
    if push:
        pb = push[0]
        if len(push) > 1 or pb.get("tag") or not pb.get("onlyIfDamaged"):
            raise Unsupported("PushBackModel(mehrfach/Tag/ohne Schaden)")
        ex["push"] = {"amount": pb["pushAmount"],
                      "factor_for": [{"tag": t, "factor": pb[k]} for t, k in PUSH_TAGS if pb.get(k, 1.0) != 1.0]}
    return camo, ex


def convert_dot(proj, slow_id):
    """AddBehaviorToBloonModel mit DamageOverTimeModel -> dot (Corrosive Glue)."""
    adds = []
    for a in behaviors(proj, "AddBehaviorToBloonModel"):
        # Glue 201 enthält denselben Effekt zweimal (nur Name/collisionPass verschieden)
        key = {k: v for k, v in a.items() if k not in ("name", "collisionPass")}
        if key not in [{k: v for k, v in x.items() if k not in ("name", "collisionPass")} for x in adds]:
            adds.append(a)
    mods = behaviors(proj, "AddBehaviorModifierForTagModel")
    if any(m["mutatorId"] == slow_id for m in mods):
        raise Unsupported("AddBehaviorModifierForTagModel für einen Slow")
    if not adds:
        # Modifier ohne passenden Effekt (MOAB Glue ohne Corrosive) wirken nicht
        return None
    if len(adds) > 1:
        raise Unsupported("mehrere AddBehaviorToBloonModel")
    ab = adds[0]
    inner = ab.get("behaviors") or []
    if len(inner) != 1 or short(inner[0]["$type"]) != "DamageOverTimeModel":
        raise Unsupported("AddBehaviorToBloonModel ohne reines DamageOverTime")
    if ab.get("filters") or ab.get("filter") or ab.get("chance", 0.0) or ab.get("applyOnlyIfDamaged") \
            or ab.get("stackCount", 0) or ab.get("cascadeMutators"):
        raise Unsupported("AddBehaviorToBloonModel(Filter/Chance/Stapel/nur bei Schaden)")
    d = inner[0]
    if d.get("triggerImmediate") or d.get("initialDelay", 0.0) or d.get("damageOnDestroy") \
            or d.get("damageModifierModels") or d.get("payloadCount", 0) or not d.get("distributeToChildren", True):
        raise Unsupported("DamageOverTimeModel(sofort/Verzögerung/Modifier)")
    # Bits ab 32 kennt das Format nicht (Spiel: 64 bei Corrosive Glue, Bedeutung unbekannt) [KAL]
    if d["immuneBloonProperties"] & 31:
        raise Unsupported("DamageOverTimeModel mit Immunität")
    return {"damage": d["damage"], "interval": d["interval"], "seconds": ab["lifespan"], "layers": int(ab["layers"]),
            "level": int(ab.get("glueLevel", 0)),
            "factor_for": [{"tag": m["tag"], "factor": m["mutatorMultiplier"]} for m in mods
                           if m["mutatorId"] == ab.get("mutationId")]}


def convert_contact(c, nested: bool) -> dict:
    """Ein CreateProjectileOn*Model -> Explosion (ohne Flugbahn) oder Splitter (mit Flugbahn).
    `nested`: innerhalb von Splittern sind nur Explosionen erlaubt."""
    if c.get("dontCreateAtBloon") or c.get("useChance") or c.get("passOnDirectionToContact"):
        raise Unsupported("CreateProjectileOn*Model(dontCreateAtBloon/Chance/Richtung)")
    sub = c["projectile"]
    check_pierce_reduction(sub)
    check_damage(sub)
    if behaviors(sub, "CreateProjectileOnContactModel"):
        raise Unsupported("verschachtelte Kontakt-Projektile")
    sub_dmg = behaviors(sub, "DamageModel")
    sub_camo, sub_ex = convert_extras(sub, all_filters(sub))
    common = {"pierce": pierce_of(sub), "damage": sub_dmg[0]["damage"] if sub_dmg else 0.0,
              "cannot_damage": cannot_damage(sub), "camo": sub_camo, "extras": sub_ex}
    travel = straight_travel(sub)
    exhaust = convert_exhaust(sub, nested=True)
    collide = not sub.get("dontUseCollisionChecker") and sub.get("canCollideWithBloons", True)
    if not collide and not (travel and exhaust):
        raise Unsupported("Projektil ohne Kollision (außer Cluster-Splitter)")
    if travel:
        if nested:
            raise Unsupported("Splitter aus Splittern")
        em = c["emission"]
        # ohne useProjectileRotation zeigen die Splitter-Richtungen nicht mit der Flugrichtung mit
        return {"kind": "shards", "emission": convert_emission(em, not em.get("useProjectileRotation", False)),
                "speed": travel[0], "lifespan": travel[1], "radius": sub["radius"],
                "inherit_hit": bool(c.get("passOnCollidedWith")), "on_exhaust": exhaust, "collide": collide,
                **common}
    if exhaust:
        raise Unsupported("Explosion mit Verbrauchs-Projektilen")
    if short(c["emission"]["$type"]) != "SingleEmissionModel":
        raise Unsupported("Explosion mit Mehrfach-Emission")
    return {"kind": "explosion", "radius": sub["radius"], **common}


def convert_contacts(proj) -> list:
    return [convert_contact(c, nested=False) for c in behaviors(proj, "CreateProjectileOnContactModel")]


def convert_exhaust(proj, nested: bool) -> list:
    """CreateProjectileOnExhaustFractionModel mit fraction 1: Effekt, sobald der Pierce verbraucht ist."""
    out = []
    for c in behaviors(proj, "CreateProjectileOnExhaustFractionModel"):
        if c.get("fraction") != 1.0 or c.get("durationfraction", -1.0) >= 0.0:
            raise Unsupported("CreateProjectileOnExhaustFractionModel(Teil-Verbrauch/Dauer)")
        out.append(convert_contact(c, nested))
    return out


def convert_weapon_behaviors(weapon, out: dict):
    """Salve und Versatz an der Waffe (STRICT, deshalb hier ausgewertet)."""
    for b in weapon.get("behaviors") or []:
        t = short(b.get("$type", ""))
        if t == "BurstWeaponBehaviorModel":
            if b.get("alwaysEmptyClip"):
                raise Unsupported("BurstWeaponBehaviorModel(alwaysEmptyClip)")
            use(b)
            out["burst"] = {"count": int(b["count"]), "interval": b["interval"]}
        elif t == "OffsetModel":
            # Bedeutung unbestätigt (Druid: range 45; Sniper-Supply-Drop: range 360, eher Zufallsdrehung).
            # Ignoriert, die mittlere Richtung bleibt gleich [KAL: echte Streuung evtl. breiter]
            if b.get("angleOffset", 0.0):
                raise Unsupported("OffsetModel(angleOffset)")
            use(b)


def check_remove_modifiers(proj):
    """RemoveBloonModifiersModel ist ohne gesetzte Schalter wirkungslos (Druid-Basis)."""
    for b in behaviors(proj, "RemoveBloonModifiersModel"):
        if any(b.get(k) for k in ("cleanseRegen", "cleanseCamo", "cleanseLead", "cleanseFortified")):
            raise Unsupported("RemoveBloonModifiersModel(entfernt Eigenschaften)")
        use(b)


def convert_homing(proj, speed: float, lifespan: float):
    """Zielsuche (Wizard: TrackTargetModel, Ninja: TrackTargetWithinTimeModel) -> game_v1 `homing`."""
    found = behaviors(proj, "TrackTargetModel") + behaviors(proj, "TrackTargetWithinTimeModel")
    if not found:
        return None
    if len(found) > 1:
        raise Unsupported("mehrere Zielsuchen")
    h = found[0]
    if h.get("constantlyAquireNewTarget") or h.get("expireOnContactTarget") or h.get("overrideRotation") \
            or h.get("turnRatePerFrame", 0.0):
        raise Unsupported(f"{short(h['$type'])}(Sonderfälle)")
    distance = speed * lifespan if h.get("useLifetimeAsDistance") else h["distance"]
    max_angle = 360.0 if h.get("ignoreSeekAngle") else min(h["maxSeekAngle"], 360.0)
    if short(h["$type"]) == "TrackTargetWithinTimeModel":
        # dreht sich innerhalb von timeInFrames (60/s) bis zur Kegelkante aufs Ziel [KAL]
        turn_rate = max_angle / 2.0 / max(h["timeInFrames"] / 60.0, 1e-6)
    else:
        turn_rate = h.get("TurnRate", h.get("turnRate"))
        if turn_rate is None:
            raise Unsupported("TrackTargetModel ohne Drehrate")
    use(h)
    return {"turn_rate": float(turn_rate), "distance": float(distance), "max_angle": float(max_angle),
            "retarget": bool(h.get("trackNewTargets"))}


def convert_instant(proj, weapon) -> dict:
    """Sofort-Treffer (Sniper, Desperado): trifft genau das Ziel ("First") ohne Flug."""
    if short(weapon["emission"]["$type"]) not in ("InstantDamageEmissionModel", "SingleEmissionModel"):
        raise Unsupported("Sofort-Treffer mit Mehrfach-Emission")
    targets_only = [f for f in (proj.get("filters") or []) + [g for pf in behaviors(proj, "ProjectileFilterModel")
                                                             for g in pf.get("filters") or []]
                    if short(f.get("$type", "")) == "FilterAllExceptTargetModel"]
    if not targets_only:
        raise Unsupported("Sofort-Treffer ohne FilterAllExceptTarget")
    if behaviors(proj, "CreateProjectileOnContactModel") or behaviors(proj, "CreateProjectileOnExhaustFractionModel"):
        raise Unsupported("Sofort-Treffer mit Kontakt-Projektilen")
    for n in behaviors(proj, "InstantModel") + targets_only + [weapon["emission"]]:
        use(n)
    return {"kind": "hitscan"}


def convert_attack(tower, attack, weapon, idx) -> dict:
    proj = weapon["projectile"]
    check_remove_modifiers(proj)
    if proj.get("dontUseCollisionChecker") or not proj.get("canCollideWithBloons", True):
        raise Unsupported("Projektil ohne Kollision")
    check_pierce_reduction(proj)
    check_damage(proj)
    dmg = behaviors(proj, "DamageModel")
    travel = straight_travel(proj)
    follow = behaviors(proj, "FollowPathModel")
    age = behaviors(proj, "AgeModel")
    clear = behaviors(proj, "ClearHitBloonsModel")
    camo, extras = convert_extras(proj, all_filters(proj, attack))
    if travel and age:
        raise Unsupported("AgeModel an einem geraden Projektil")
    instant = behaviors(proj, "InstantModel")
    if instant and (travel or follow):
        raise Unsupported("InstantModel mit Flugbahn")
    if instant:
        delivery = convert_instant(proj, weapon)
    elif travel or follow:
        emission = convert_emission(weapon["emission"], not rotates(tower))
        if follow:
            fp = follow[0]
            if fp.get("easePath") or not fp.get("destroyAtEndOfPath", True):
                raise Unsupported("FollowPathModel(easePath/bleibt am Ende)")
            path = [[v["x"], v["y"]] for v in fp["path"]]
            lifespan = age[0]["lifespan"] if age else 60.0
            delivery = {"kind": "projectile", "speed": fp["speed"], "lifespan": lifespan, "radius": proj["radius"],
                        "emission": emission, "path": path}
        else:
            delivery = {"kind": "projectile", "speed": travel[0], "lifespan": travel[1],
                        "radius": proj["radius"], "emission": emission}
            homing = convert_homing(proj, travel[0], travel[1])
            if homing:
                delivery["homing"] = homing
        delivery["on_contact"] = convert_contacts(proj)
        delivery["on_exhaust"] = convert_exhaust(proj, nested=False)
        if clear:
            delivery["clear_hits"] = clear[0]["interval"]
    else:
        # kein Flug: Flächentreffer um den Turm (Ice, Ring of Fire). Der Radius steht im Projektil
        # (LinkProjectileRadiusToTowerRange hat ihn im Dump schon auf Reichweite + Versatz gesetzt).
        if behaviors(proj, "CreateProjectileOnContactModel") or behaviors(proj, "CreateProjectileOnExhaustFractionModel"):
            raise Unsupported("Flächentreffer mit Kontakt-Projektilen")
        if short(weapon["emission"]["$type"]) != "SingleEmissionModel":
            raise Unsupported("Flächentreffer mit Mehrfach-Emission")
        if clear:
            raise Unsupported("Flächentreffer mit ClearHitBloons")
        link = behaviors(proj, "LinkProjectileRadiusToTowerRangeModel")
        if link and abs(proj["radius"] - (attack["range"] + link[0].get("projectileRadiusOffset", 0.0))) > 1e-3:
            raise Unsupported("LinkProjectileRadiusToTowerRange passt nicht zum Radius")
        delivery = {"kind": "area", "radius": proj["radius"]}
    crit = [b for b in weapon.get("behaviors") or [] if short(b.get("$type", "")) == "CritMultiplierModel"]
    out = {
        "tag": f"{attack.get('name', 'attack')}-{idx}".replace(" ", "_"),
        "range": attack["range"] if attack["range"] != tower["range"] else None,
        "cooldown": weapon["rate"],
        "damage": dmg[0]["damage"] if dmg else 0.0,
        "pierce": 1 if instant else pierce_of(proj),  # Sofort-Treffer: nur das Ziel
        "cannot_damage": cannot_damage(proj),
        "camo": camo,
        "delivery": delivery,
        "extras": extras,
    }
    convert_weapon_behaviors(weapon, out)
    if crit:
        c = crit[0]
        # Abstand gleichverteilt in [lower, upper] (mit Zufall in der Sim), sonst der gerundete Mittelwert
        # [KAL: ob das Spiel upper einschließt (Unity Random.Range für ints schließt es aus)]
        out["crit"] = {"damage": c["damage"], "every": max(1, round((c["lower"] + c["upper"]) / 2)),
                       "every_range": [max(1, int(c["lower"])), max(1, int(c["upper"]))]}
    return out


def convert_variant(model, tiers) -> dict:
    _used.clear()  # id() gilt nur, solange das Modell lebt
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
    check_strict(model)
    for a in attacks:
        if a["range"] is None:
            del a["range"]
    zones = []
    for z in behaviors(model, "SlowBloonsZoneModel"):
        if z.get("speedChange", 0.0) or z.get("bloonTag") or z.get("bloonTags"):
            raise Unsupported("SlowBloonsZoneModel(speedChange/Tags)")
        camo, filt = convert_filters(z.get("filters") or [])
        radius = model["range"] + z.get("radiusOffset", 0.0) if z.get("bindRadiusToTowerRange") else z["zoneRadius"]
        zones.append({"radius": radius, "speed_scale": z["speedScale"], "camo": camo, "filter": filt})
    v = {"tiers": tiers, "range": model["range"], "footprint": model["footprint"]["radius"], "attacks": attacks}
    if zones:
        v["zones"] = zones
    return v


TAG_LISTS = ("damage_vs", "scale", "multiplier_for", "seconds_for", "factor_for")


def prune_tags(node, known: set, dropped: set):
    """Entfernt Tags, die kein Bloon der Konfiguration hat (z.B. "Boss" auf Monkey Meadow): game_v1
    lehnt unbekannte Tags ab, damit Tippfehler auffallen. Ordnet Tag-Listen nach Vorrang."""
    if isinstance(node, dict):
        for k, v in node.items():
            if k == "exclude_tags":
                dropped.update(t for t in v if t not in known)
                v[:] = [t for t in v if t in known]
            elif k in TAG_LISTS and isinstance(v, list):
                dropped.update(e["tag"] for e in v if e["tag"] not in known)
                # die Engine nimmt den ersten passenden Eintrag: die Klasse "Moabs" zuletzt, damit
                # z.B. "Bad" (BAD hat auch den Tag "Moabs") Vorrang hat
                v[:] = sorted((e for e in v if e["tag"] in known), key=lambda e: e["tag"] == "Moabs")
            else:
                prune_tags(v, known, dropped)
    elif isinstance(node, list):
        for v in node:
            prune_tags(v, known, dropped)


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
            # Dateinamen im Dump: ":" ist durch "_" ersetzt (z.B. "Necromancer_ Unpopped Army")
            costs[p].append(int(load(dump / "upgrades" / f"{name.replace(':', '_')}.json")["cost"]))
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
    ap.add_argument("--towers", choices=sorted(TOWER_SETS), default="primary",
                    help="Turm-Set; außer primary bekommt die Ausgabe den Namen als Suffix")
    args = ap.parse_args()
    dump = args.dump or latest_dump()
    preset = PRESETS[args.preset]

    coverage = []
    towers = [build_tower(dump, t, coverage) for t in TOWER_SETS[args.towers]]
    footprint = max(v["footprint"] for t in towers for v in t["variants"])
    last = preset["end_round"]
    rounds = build_rounds(dump, args.rounds, last)
    needed = {g["bloon"] for r in rounds for g in r}
    speed = bloon_speed_factor(dump, preset["match"], args.map)
    bloons = build_bloons(dump, needed, speed)
    dropped: set = set()
    prune_tags(towers, {t for b in bloons for t in b["tags"]}, dropped)
    data = {
        "format": "game_v1",
        "name": f"{args.map}-{args.preset}-{dump.name}",
        "rules": {"tick_rate": 60.0, "cd_eps": 1e-9, "max_round_seconds": 900.0, "child_spacing": 0.0,
                  "sell_ratio": 0.7, "cost_rounding": 5, **{k: v for k, v in preset.items() if k != "match"}},
        "map": build_map(dump, args.map, footprint),
        "bloons": bloons,
        "towers": towers,
        "rounds": rounds,
        "income": build_income(dump, args.rounds, last),
    }
    OUT.mkdir(parents=True, exist_ok=True)
    stem = f"game_v1_{args.map.lower()}_{args.preset}" + ("" if args.towers == "primary" else f"_{args.towers}")
    (OUT / f"{stem}.json").write_text(json.dumps(data, indent=1) + "\n")

    lines = [f"# Abdeckung {stem} (Dump {dump.name})", "",
             "Erzeugt von tools/build_game_v1.py. Nicht unterstützte Kombinationen sind im Aktionsraum gesperrt.", "",
             "Bekannte Vereinfachungen der Sim (gelten für alle Kombinationen):",
             "- Keine Sichtlinie: Bäume/Blocker halten im Spiel Schüsse auf, in der Sim nicht.",
             "- Targeting immer \"First\"; Explosionen wirken sofort statt 0,1 s.",
             "- Fähigkeiten (AbilityModel) sind bewusst nicht simuliert (Design: M4); "
             "Kombinationen damit bleiben gesperrt.",
             "- Projektile starten in der Turmmitte (Abwurf-Versatz der Waffe ignoriert); Boomerang-Kurven "
             "werden nicht gespiegelt (FlipFollowPath).",
             "- Zufall durch Mittelwerte ersetzt: Cluster-Reichweite, Crit-Intervall.",
             f"- Tags ohne Bloon in dieser Konfiguration entfernt: {', '.join(sorted(dropped)) or '-'}.", ""]
    for base in TOWER_SETS[args.towers]:
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
