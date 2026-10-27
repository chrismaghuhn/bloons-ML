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
# docs/design-towers-step1.md, t14 = dazu Village und Farm (docs/design-towers-step2.md), t21 = dazu die
# Landtürme aus docs/design-towers-step3.md
T12 = TOWERS + ["SniperMonkey", "Desperado", "SuperMonkey", "Druid", "NinjaMonkey", "WizardMonkey"]
T14 = T12 + ["MonkeyVillage", "BananaFarm"]
T21 = T14 + ["Alchemist", "EngineerMonkey", "MortarMonkey", "DartlingGunner", "SpikeFactory", "HeliPilot",
             "MonkeyAce"]
TOWER_SETS = {"primary": TOWERS, "t12": T12, "t14": T14, "t21": T21}
TOWER_NAMES = {"DartMonkey": "dart", "BoomerangMonkey": "boomerang", "BombShooter": "bomb",
               "TackShooter": "tack", "IceMonkey": "ice", "GlueGunner": "glue", "SniperMonkey": "sniper",
               "Desperado": "desperado", "SuperMonkey": "super", "Druid": "druid", "NinjaMonkey": "ninja",
               "WizardMonkey": "wizard", "MonkeyVillage": "village", "BananaFarm": "farm",
               "Alchemist": "alchemist", "EngineerMonkey": "engineer", "MortarMonkey": "mortar",
               "DartlingGunner": "dartling", "SpikeFactory": "spike", "HeliPilot": "heli", "MonkeyAce": "ace"}
# towerSet des Spiels -> game_v1
TOWER_SET_NAMES = {1: "primary", 2: "military", 4: "magic", 8: "support"}
# Türme, deren Mechanik die Engine noch nicht kann: in allen Presets gesperrt (rules.locked_towers),
# aber schon in Daten und Layout, damit sich der Aktionsraum später nicht ändert
# (docs/design-towers-step2.md §2.4). Geht ihre Basisvariante noch nicht durch den Konverter, steht sie als
# Platzhalter in den Daten (docs/design-towers-step3.md §4).
MECHANIC_LOCKED = {"BananaFarm", "DartlingGunner", "SpikeFactory", "HeliPilot", "MonkeyAce"}

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
           # GrowBlock am Frost: gefrorene Bloons wachsen in der Sim ohnehin nicht nach (frozen > 0);
           # ohne Wirkung in der Sim:
           # Drehung/Spiegelung nur Darstellung [KAL: FlipFollowPath spiegelt evtl. die Kurve],
           # Wasser einfrieren gibt es auf Monkey Meadow nicht
           "GrowBlockModel", "RotateModel", "FlipFollowPathModel", "FreezeNearbyWaterModel",
           # M2c: Zufallsreichweite, Verbrauchs-Projektile (Cluster), Schaden über Zeit, Zurückstoßen,
           # Treffer-Reset, kritische Treffer, parallele Emission, Verlangsamungs-Zone
           "RandomRangeTravelStraitModel", "CreateProjectileOnExhaustFractionModel", "AddBehaviorToBloonModel",
           "DamageOverTimeModel", "AddBehaviorModifierForTagModel", "PushBackModel", "ClearHitBloonsModel",
           "CritMultiplierModel", "ParallelEmissionModel", "SlowBloonsZoneModel",
           # Türme Schritt 1: Zielen ohne Abwurf-Versatz (die Sim zielt ohnehin von der Turmmitte)
           "CheckTargetsWithoutOffsetsModel",
           # Türme Schritt 3 (docs/design-towers-step3.md): Abwurfpunkte (wie ejectX/ejectY ignoriert; die
           # Emission selbst wertet convert_emission aus), Kartenobjekt einer anderen Karte (Mortar-Explosion),
           # Pin (Engineer xx2: SlowOnPop = 1 s Stopp der Kinder beim Pop; Status-Effekte beim Pop kann die Engine
           # noch nicht, deshalb ohne Wirkung [KAL: auch `chance` 0 – immer oder nie?])
           "EmissionWithOffsetsModel", "ThrowMarkerOffsetModel", "ChipMapBasedObjectModel", "SlowOnPopModel",
           # Alchemist: Pfütze nur, wenn der Spritzer nichts trifft (Lebensdauer 0,05 s) [KAL]
           "AcidPoolModel"}

# Nur dort erlaubt, wo der Konverter sie wirklich auswertet (`use`); jedes andere Vorkommen (z.B. in
# einem Kontakt-Projektil eines späteren Upgrades) macht die Variante "nicht unterstützt".
STRICT = {"InstantModel", "InstantDamageEmissionModel", "FilterAllExceptTargetModel", "BurstWeaponBehaviorModel",
          "RandomArcEmissionModel", "RandomEmissionModel", "OffsetModel", "RemoveBloonModifiersModel",
          "TrackTargetModel", "TrackTargetWithinTimeModel",
          # Türme Schritt 3: Wurf auf einen festen Punkt (convert_point_lob)
          "TargetSelectedPointModel", "RotateToPointerModel", "RandomTargetSpreadModel", "FilterAllModel",
          # Alchemist-Tränke (convert_potion)
          "TargetFriendlyModel", "BrewTargettingModel", "AddAcidicMixtureToProjectileModel", "AcidicMixtureModel",
          "AcidicMixtureCheckModel", "AddBerserkerBrewToProjectileModel", "BerserkerBrewModel",
          "BerserkerBrewCheckModel", "DestroyIfTargetLostModel",
          # Buff-Anzeige; nur die der Alchemist-Tränke ist ausgewertet (Super Monkey 3xx nutzt sie für den Tempel)
          "CanBuffIndicatorModel"}
# Buff-Anzeigen (CanBuffIndicatorModel.buffLocsName) ohne Wirkung: die der Alchemist-Tränke
POTION_INDICATORS = {"AcidicMixtureBuff", "BeserkerBrewBuff"}
# Türme Schritt 2 (docs/design-towers-step2.md): Auren und Rabatt (convert_supports), Einkommen
# (convert_income) und Footprint (convert_footprint); alle nur dort erlaubt, wo sie ausgewertet werden
SUPPORT_MODELS = {"RangeSupportModel", "RateSupportModel", "VisibilitySupportModel", "PierceSupportModel",
                  "ProjectileSpeedSupportModel", "DamageTypeSupportModel"}
INCOME_MODELS = {"CashModel", "EmissionsPerRoundFilterModel", "WeaponRateMinModel", "RandomPositionBasicModel",
                 "SingleEmmisionTowardsTargetModel", "ArriveAtTargetModel", "HeightOffsetProjectileModel",
                 "PickupModel", "ScaleProjectileModel"}
STRICT |= SUPPORT_MODELS | INCOME_MODELS | {
    "SupportRemoveFilterOutTagModel", "FilterInSetModel", "DiscountZoneModel", "AddBehaviorToBloonInZoneModel",
    "RectangleFootprintModel", "PerRoundCashBonusTowerModel", "BonusLivesPerRoundModel", "CollectCashZoneModel",
    "CentralMarketBuffModel"}
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
    "medium": dict(start_cash=650, start_lives=150, start_round=1, end_round=40,
                   cost_multiplier=1.0, match="Standard_Standard_Medium"),
    # Medium bis zum regulären Ende; Benchmark (b): Runde 1-60
    "medium60": dict(start_cash=650, start_lives=150, start_round=1, end_round=60,
                     cost_multiplier=1.0, match="Standard_Standard_Medium"),
    # CHIMPS (Werte aus dem Dump: matches/Standard_Clicks_Hard_Tutorial.json)
    "chimps": dict(start_cash=650, start_lives=1, start_round=6, end_round=100,
                   cost_multiplier=1.08, match="Standard_Clicks_Hard"),
}
# Verkaufen und Turm-Sperren kommen aus den Mods der Partie (match_rules); Verkaufsquote fest
SELL_RATIO = 0.7

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


def match_rules(dump: Path, match: str, map_name: str) -> tuple:
    """Verkaufen erlaubt? und gesperrte Türme (Basis-IDs) aus den Mods der Partie: DisableSellTowerModModel
    (gegengeprüft mit dem Partie-Feld towerSellEnabled) und LockTowerModModel (CHIMPS: Banana Farm).
    Andere Sperr-Mods (z.B. LockTowerSetModModel) kann der Konverter nicht und bricht ab."""
    path = dump / "matches" / f"{match}_{map_name}.json"
    if not path.exists():
        sys.exit(f"{path} fehlt: einmal eine Partie in diesem Modus mit der Mod starten")
    m = load(path)
    selling, locked = True, []

    def walk(n):
        nonlocal selling
        if isinstance(n, dict):
            t = short(n.get("$type", ""))
            if t == "DisableSellTowerModModel":
                selling = False
            elif t == "LockTowerModModel":
                if n["towerToLock"] not in locked:
                    locked.append(n["towerToLock"])
            elif t.endswith("ModModel") and "Lock" in t:
                sys.exit(f"{path}: Sperr-Mod {t} nicht unterstützt")
            for v in n.values():
                walk(v)
        elif isinstance(n, list):
            for v in n:
                walk(v)
    walk(m.get("mods", []))
    fields = m.get("fields") or {}
    enabled = fields.get("towerSellEnabled")
    if enabled is not None and enabled != selling:
        sys.exit(f"{path}: towerSellEnabled={enabled} passt nicht zu den Mods (Verkaufen {selling})")
    if selling and fields.get("sellMultiplier", SELL_RATIO) != SELL_RATIO:
        sys.exit(f"{path}: sellMultiplier={fields['sellMultiplier']}, die Sim verkauft fest zu {SELL_RATIO}")
    return selling, locked


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

    def grow(model) -> dict:
        g = [b for b in model.get("behaviors") or [] if short(b.get("$type", "")) == "GrowModel"]
        return g[0] if g else {}

    models = {n: load(dump / "bloons" / summary[n]["baseId"] / f"{n}.json") for n in sorted(seen)}
    check_regrow(summary, models, grow)
    out = []
    for n in sorted(seen):
        s, model = summary[n], models[n]
        out.append({
            "name": n, "hp": float(s["maxHealth"]), "speed": float(s["speed"]) * speed_factor,
            "leak": int(round(leak(n))),
            "children": children_counts(s.get("children")),
            "props": props(s["bloonProperties"]),
            "classes": (["moab"] if s["isMoab"] else []) + (["ceramic"] if s["baseId"] == "Ceramic" else [])
                       + (["boss"] if s["isBoss"] else []),
            "camo": bool(s["isCamo"]), "regrow": bool(s["isGrow"]), "radius": float(model["radius"]),
            "tags": list(s.get("tags") or []),
            **({"grow_rate": float(grow(model)["rate"])} if s["isGrow"] else {}),
        })
    return out


def check_regrow(summary: dict, models: dict, grow) -> None:
    """Regrow simuliert die Engine über die Vorfahren (docs/design-bloons-chimps.md §2). Hier nur prüfen,
    dass das zum Spiel passt: Jedes Regrow-Kind eines Regrow-Elternteils wächst laut `growToId` zum
    Elternteil (Vergleich ohne Fortified) oder der Elternteil setzt `overrideChildWithColor` auf die
    Farbe des Kinds. Außerdem hat jeder Regrow-Typ ein `GrowModel` mit positiver Rate."""
    def plain(n: str) -> str:
        return n.replace("Fortified", "")

    for n, model in models.items():
        s = summary[n]
        if not s["isGrow"]:
            continue
        g = grow(model)
        if not g or not g.get("rate", 0) > 0:
            raise Unsupported(f"Bloon {n}: Regrow ohne GrowModel/Rate")
        for c in s.get("children") or []:
            if not summary[c]["isGrow"]:
                raise Unsupported(f"Bloon {n}: Kind {c} ohne Regrow")
            to = grow(models[c]).get("growToId") or ""
            if plain(to) != plain(n) and g.get("overrideChildWithColor") != summary[c]["baseId"]:
                raise Unsupported(f"Bloon {n}: Kind {c} wächst zu {to or '-'}, nicht zum Elternteil")


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
    if et == "EmissionWithOffsetsModel":
        # Projektile über Abwurfpunkte verteilt: eins je Schuss wechselt nur den Abwurfpunkt (Engineer Double
        # Gun), eins je Punkt = nebeneinander mit dem Abstand der Punkte quer zur Schussrichtung
        markers = em.get("throwMarkerOffsetModels") or []
        n = em.get("projectileCount", 1)
        if em.get("randomRotationCone", 0.0) or not em.get("rotateProjectileWithTower", True):
            raise Unsupported("EmissionWithOffsetsModel(Zufallskegel/feste Richtung)")
        if n == 1:
            return {"kind": "single"}
        xs = sorted(m["ejectX"] for m in markers)
        if n != len(markers) or not em.get("spreadProjectilesAcrossMarkers") \
                or any(abs(a + b) > 1e-6 for a, b in zip(xs, reversed(xs))):
            raise Unsupported("EmissionWithOffsetsModel(mehrere Projektile je Punkt/asymmetrisch)")
        gaps = {round(b - a, 6) for a, b in zip(xs, xs[1:])}
        if len(gaps) != 1:
            raise Unsupported("EmissionWithOffsetsModel(ungleiche Abstände)")
        return {"kind": "parallel", "count": n, "spread": gaps.pop()}
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
        # 9 999 999 (Mortar-Explosion) heißt unbegrenzt
        if 0.0 < d.get("maxDamage", 0.0) < 1e6:
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
            # MIB (Village x3x) schaltet diese Filter ab; das kann die Sim noch nicht
            if MIB_MUTATOR in (x.get("disableWhenSupportMutatorIDs") or []):
                raise Unsupported("FilterOutTagModel, den MIB abschaltet")
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


def convert_point_lob(tower, attack, weapon, idx) -> dict:
    """Wurf auf einen festen Punkt (Mortar): Granate ohne Treffer im Flug (FilterAll, InstantModel), Explosion
    nach der Lebensdauer am Zielpunkt. Ziel ist `track_best` statt des gewählten Punkts, Streuung als `spread`
    (docs/design-towers-step3.md §2.1/§3)."""
    proj, em = weapon["projectile"], weapon["emission"]
    point = behaviors(attack, "TargetSelectedPointModel")
    instant, age = behaviors(proj, "InstantModel"), behaviors(proj, "AgeModel")
    if len(point) != 1 or len(instant) != 1 or len(age) != 1:
        raise Unsupported("Zielpunkt-Angriff ohne InstantModel/AgeModel")
    if point[0].get("isOnSubTower") or point[0].get("projectileToExpireOnTargetChangeModel") \
            or em.get("throwMarkerOffsets"):
        raise Unsupported("Zielpunkt-Angriff auf Sub-Turm/mit Ablauf bei Zielwechsel/Abwurfpunkten")
    if point[0].get("startWithClosestTrackPoint") or point[0].get("lockToInsideTowerRange"):
        raise Unsupported("TargetSelectedPointModel(nächster Streckenpunkt/innerhalb der Reichweite)")
    if short(em["$type"]) != "RandomTargetSpreadModel":
        raise Unsupported(f"Zielpunkt-Angriff mit Emission {short(em['$type'])}")
    if age[0].get("rounds") or age[0].get("useRoundTime") or instant[0].get("dontFollowTarget"):
        raise Unsupported("Zielpunkt-Angriff mit AgeModel(rounds/useRoundTime)/InstantModel(dontFollowTarget)")
    blocks = all_filters(proj)
    if not blocks or any(short(f.get("$type", "")) != "FilterAllModel" for f in blocks):
        raise Unsupported("Zielpunkt-Angriff mit Treffern im Flug")
    if behaviors(proj, "DamageModel") or behaviors(proj, "CreateProjectileOnContactModel") or straight_travel(proj):
        raise Unsupported("Zielpunkt-Angriff mit Schaden/Kontakt/Flugbahn der Granate")
    on_arrive = convert_exhaust(proj, nested=False)
    if not on_arrive or any(c["kind"] != "explosion" for c in on_arrive):
        raise Unsupported("Zielpunkt-Angriff ohne Explosion")
    camo, extras = convert_extras({"behaviors": []}, all_filters(attack))
    for n in point + instant + age + blocks + [em] + behaviors(attack, "RotateToPointerModel"):
        use(n)
    out = {
        "tag": f"{attack.get('name', 'attack')}-{idx}".replace(" ", "_"),
        "range": attack["range"] if attack["range"] != tower["range"] else None,
        "targeting": "track_best",
        "cooldown": weapon["rate"],
        "damage": 0.0,
        "pierce": 1,
        "cannot_damage": [],
        "camo": camo,
        "delivery": {"kind": "lob", "time": age[0]["lifespan"], "spread": em["spread"], "on_arrive": on_arrive},
        "extras": extras,
    }
    convert_weapon_behaviors(weapon, out)
    return out


def arrive_flight(proj):
    """ArriveAtTargetModel (Flug zum Zielpunkt ohne Treffer) -> (time, speed) für `lob`, sonst None."""
    arrive = behaviors(proj, "ArriveAtTargetModel")
    if not arrive:
        return None
    a = arrive[0]
    if len(arrive) > 1 or not a.get("filterCollisionWhileMoving") or a.get("keepUpdatingTargetPos") \
            or a.get("rotateToTarget") or a.get("maxTurnAngle", 0.0):
        raise Unsupported("ArriveAtTargetModel(Treffer im Flug/folgt dem Ziel)")
    for n in arrive + behaviors(proj, "ScaleProjectileModel") + behaviors(proj, "HeightOffsetProjectileModel"):
        use(n)  # Kurve, Größe und Höhe sind Darstellung
    if a.get("altSpeed", 0.0) > 0.0:
        return None, a["altSpeed"]
    if a.get("timeToTake", 0.0) > 0.0:
        return a["timeToTake"], None
    raise Unsupported("ArriveAtTargetModel ohne Tempo/Zeit")


def convert_arrive_lob(tower, attack, weapon, idx, flight) -> dict:
    """Wurf auf die Position des Ziels beim Abwurf (Alchemist): Trank ohne Treffer im Flug, platzt bei Ankunft
    (docs/design-towers-step3.md §2.1)."""
    proj, a = weapon["projectile"], behaviors(weapon["projectile"], "ArriveAtTargetModel")[0]
    if not a.get("expireOnArrival") or a.get("stopOnTargetReached"):
        raise Unsupported("ArriveAtTargetModel(bleibt liegen)")
    if short(weapon["emission"]["$type"]) != "SingleEmissionModel":
        raise Unsupported("Wurf mit Mehrfach-Emission")
    if not proj.get("dontUseCollisionChecker") or behaviors(proj, "DamageModel") \
            or behaviors(proj, "CreateProjectileOnContactModel") or straight_travel(proj):
        raise Unsupported("Wurf mit Treffern/Schaden/Kontakt/Flugbahn")
    on_arrive = convert_exhaust(proj, nested=False)
    if not on_arrive or any(c["kind"] != "explosion" for c in on_arrive):
        raise Unsupported("Wurf ohne Explosion")
    for n in behaviors(proj, "AgeModel"):
        use(n)  # Höchstdauer; der Flug endet bei Ankunft (Reichweite / Tempo ist viel kürzer)
    camo, extras = convert_extras({"behaviors": []}, all_filters(attack))
    time, speed = flight
    delivery = {"kind": "lob", "on_arrive": on_arrive}
    delivery.update({"time": time} if time is not None else {"speed": speed})
    out = {
        "tag": f"{attack.get('name', 'attack')}-{idx}".replace(" ", "_"),
        "range": attack["range"] if attack["range"] != tower["range"] else None,
        "cooldown": weapon["rate"],
        "damage": 0.0,
        "pierce": 1,
        "cannot_damage": [],
        "camo": camo,
        "delivery": delivery,
        "extras": extras,
    }
    convert_weapon_behaviors(weapon, out)
    return out


def bonus_of(m) -> dict:
    """DamageModifierForTagModel -> game_v1-Bonus (mustIncludeAllTags: erster Tag + `all`)."""
    if m.get("damageMultiplier", 1.0) != 1.0 or m.get("ignoreTag") or m.get("applyOverMaxDamage"):
        raise Unsupported("DamageModifierForTagModel(Multiplikator/Tag-Logik)")
    tags = m.get("tags") or [m["tag"]]
    if len(tags) > 1 and not m.get("mustIncludeAllTags"):
        raise Unsupported("DamageModifierForTagModel(mehrere Tags, einer genügt)")
    out = {"tag": tags[0], "add": m["damageAddative"]}
    if len(tags) > 1:
        out["all"] = tags[1:]
    return out


def tower_names(ids) -> list:
    """Spiel-IDs -> game_v1-Namen (unbekannte Türme fallen weg; main() kürzt auf das Turm-Set)."""
    return sorted({TOWER_NAMES[i] for i in ids if i in TOWER_NAMES})


def convert_potion(tower, attack, weapon, idx) -> dict:
    """Trank auf einen befreundeten Turm (Acidic Mixture Dip, Berserker Brew) -> Zeit-Effekt
    (docs/design-towers-step3.md §2.4)."""
    proj = weapon["projectile"]
    friendly, brew = behaviors(attack, "TargetFriendlyModel"), behaviors(attack, "BrewTargettingModel")
    provider = attack.get("targetProvider") or {}
    if len(friendly) + len(brew) != 1 or short(provider.get("$type", "")) not in ("TargetFriendlyModel",
                                                                                 "BrewTargettingModel"):
        raise Unsupported("Trank ohne eindeutige Zielwahl")
    target = (friendly + brew)[0]
    if short(provider["$type"]) != short(target["$type"]) or target.get("isSelectable"):
        raise Unsupported("Trank mit wählbarer/abweichender Zielwahl")
    if target.get("isOnSubTower") or (friendly and not target.get("mustHaveWeapon")):
        raise Unsupported("Trank auf Sub-Türme/Türme ohne Waffe")
    if short(weapon["emission"]["$type"]) != "SingleEmmisionTowardsTargetModel":
        raise Unsupported("Trank mit anderer Emission")
    only_target = [f for f in all_filters(proj) if short(f.get("$type", "")) == "FilterAllExceptTargetModel"]
    if len(only_target) != len(all_filters(proj)) or not only_target:
        raise Unsupported("Trank mit Treffern auf Bloons")
    arrive_flight(proj)  # Flugzeit unter 0,2 s wird ignoriert [KAL]
    dip, add = behaviors(proj, "AddAcidicMixtureToProjectileModel"), behaviors(proj, "AddBerserkerBrewToProjectileModel")
    if len(dip) + len(add) != 1 or behaviors(proj, "DamageModel") or behaviors(proj, "CreateProjectileOnExhaustFractionModel"):
        raise Unsupported("Trank ohne genau einen Effekt")
    if dip:
        d = dip[0]
        checks = d.get("towerBehaviors") or []
        if any(short(c["$type"]) != "AcidicMixtureCheckModel" or c.get("mutationId") != d["mutationId"]
               for c in checks) or target.get("mutationId", d["mutationId"]) != d["mutationId"] \
                or any(short(c["$type"]) != "AcidicMixtureModel" for c in d.get("weapBehaviors") or []):
            raise Unsupported("AddAcidicMixtureToProjectileModel mit anderen Turm-/Waffen-Effekten")
        bonus = []
        for m in d.get("projBehaviors") or []:
            if short(m["$type"]) != "DamageModifierForTagModel":
                raise Unsupported(f"AddAcidicMixtureToProjectileModel mit {short(m['$type'])}")
            bonus.append(bonus_of(m))
        # `cap` (30) als Zahl der Schüsse; `maxCount` (10) der Prüfung ist unklar [KAL]
        # Ausschlüsse stehen je nach Variante am Trank oder an der Zielwahl: beide zählen
        ignore = (d.get("ignoreList") or "").split(",") + (target.get("ignoreList") or "").split(",")
        effect = {"group": d["mutationId"], "shots": int(d["cap"]), "damage_vs": bonus,
                  "exclude": tower_names(ignore)}
        used = [d] + checks + (d.get("weapBehaviors") or []) + (d.get("projBehaviors") or [])
    else:
        d = add[0]
        order = d.get("ignoreMutationsByOrder") or [d["mutationId"]]
        checks = d.get("towerBehaviors") or []
        if d["mutationId"] not in order or any(short(c["$type"]) != "BerserkerBrewCheckModel" for c in checks) \
                or any(short(c["$type"]) != "BerserkerBrewModel" for c in d.get("weapBehaviors") or []):
            raise Unsupported("AddBerserkerBrewToProjectileModel(Reihenfolge/andere Effekte)")
        rank = order.index(d["mutationId"])
        # Annahmen prüfen: ersetzt die schwächeren Stufen, Zielwahl und Zähler passen zum Trank, Werte endlich
        # (Permanent Brew hat lifespan -1 und cap 9 999 999)
        if (d.get("mutatorsToRemoveList") or []) != order[:rank] \
                or (target.get("ignoreMutationsByOrder") or order) != order \
                or any(c.get("maxCount") != d["cap"] or c.get("mutationId") != d["mutationId"] for c in checks) \
                or d["lifespan"] <= 0 or d.get("rebuffBlockTime", 0.0) < 0 or d.get("lifespanFrames") \
                or d.get("rebuffBlockTimeFrames") or d["pierceUp"] != int(d["pierceUp"]):
            raise Unsupported("AddBerserkerBrewToProjectileModel(Stufen/Zähler/Dauer)")
        effect = {"group": order[0], "rank": rank, "seconds": d["lifespan"],
                  "shots": int(d["cap"]), "block": d.get("rebuffBlockTime", 0.0), "damage": d["damageUp"],
                  "pierce": int(d["pierceUp"]), "rate_frac": d["rateUp"], "range_frac": d["rangeUp"],
                  "exclude": tower_names((d.get("ignoreList") or "").split(",")
                                         + (target.get("towerIgnoreList") or []))}
        used = [d] + (d.get("towerBehaviors") or []) + (d.get("weapBehaviors") or [])
    weapon_fx = [b for b in weapon.get("behaviors") or [] if short(b.get("$type", "")) in
                 ("AcidicMixtureModel", "BerserkerBrewModel")]
    for n in used + only_target + [target, provider, weapon["emission"]] + weapon_fx \
            + behaviors(proj, "DestroyIfTargetLostModel") + behaviors(proj, "AgeModel"):
        use(n)
    return {
        "tag": f"{attack.get('name', 'attack')}-{idx}".replace(" ", "_"),
        "range": attack["range"] if attack["range"] != tower["range"] else None,
        "targeting": "friendly",
        "cooldown": weapon["rate"],
        "damage": 0.0,
        "pierce": 1,
        "cannot_damage": [],
        "camo": False,
        "delivery": {"kind": "potion", "effect": effect},
        "extras": {},
    }


def convert_attack(tower, attack, weapon, idx) -> dict:
    proj = weapon["projectile"]
    if behaviors(attack, "TargetSelectedPointModel"):
        return convert_point_lob(tower, attack, weapon, idx)
    if behaviors(attack, "TargetFriendlyModel") or behaviors(attack, "BrewTargettingModel"):
        return convert_potion(tower, attack, weapon, idx)
    flight = arrive_flight(proj)
    if flight:
        return convert_arrive_lob(tower, attack, weapon, idx, flight)
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
        # (LinkProjectileRadiusToTowerRange am Turm hat ihn im Dump schon auf Reichweite + Versatz gesetzt).
        if behaviors(proj, "CreateProjectileOnContactModel") or behaviors(proj, "CreateProjectileOnExhaustFractionModel"):
            raise Unsupported("Flächentreffer mit Kontakt-Projektilen")
        if short(weapon["emission"]["$type"]) != "SingleEmissionModel":
            raise Unsupported("Flächentreffer mit Mehrfach-Emission")
        if clear:
            raise Unsupported("Flächentreffer mit ClearHitBloons")
        link = [lk for lk in behaviors(tower, "LinkProjectileRadiusToTowerRangeModel")
                if (lk.get("projectileModel") or {}).get("id") == proj.get("id")]
        if len(link) > 1:
            raise Unsupported("mehrere LinkProjectileRadiusToTowerRange für ein Projektil")
        if link and abs(proj["radius"] - (attack["range"] + link[0].get("projectileRadiusOffset", 0.0))) > 1e-3:
            raise Unsupported("LinkProjectileRadiusToTowerRange passt nicht zum Radius")
        delivery = {"kind": "area", "radius": proj["radius"]}
        if link:
            # gekoppelt an die Reichweite: zählt erst mit Reichweiten-Buffs (Village)
            delivery["radius_offset"] = link[0].get("projectileRadiusOffset", 0.0)
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


MIB_MUTATOR = "MibMutator"
# Felder, die bei allen Auren-Modellen aus sein müssen (sonst andere Reichweite/Ziele als die Village)
SUPPORT_OFF = ("isGlobal", "isCustomRadius", "appliesToOwningTower", "onlyAffectParagon")


def support_sets(sup) -> list:
    """Turmgruppen-Filter einer Aura (FilterInSetModel) -> game_v1 `sets` (leer = alle Türme)."""
    sets = []
    for f in sup.get("filters") or []:
        if short(f.get("$type", "")) != "FilterInSetModel":
            raise Unsupported(f"Support-Filter {short(f.get('$type', ''))}")
        use(f)
        for s in f.get("towerSets") or [f["towerSet"]]:
            if s not in TOWER_SET_NAMES:
                raise Unsupported(f"towerSet {s}")
            if TOWER_SET_NAMES[s] not in sets:
                sets.append(TOWER_SET_NAMES[s])
    return sets


def convert_supports(model) -> tuple:
    """Auren und Rabatt-Zonen am Turm (Village) -> (supports, discount). Radius ist immer die
    Reichweite des Turms; Stapeln über `group` (= mutatorId, docs/design-towers-step2.md §1.1)."""
    supports, discount = [], []
    for b in model.get("behaviors") or []:
        t = short(b.get("$type", ""))
        if t in SUPPORT_MODELS:
            if any(b.get(k) for k in SUPPORT_OFF) or b.get("maxStacks") or b.get("maxStackSize"):
                raise Unsupported(f"{t} (global/eigener Radius/stapelbar)")
            if not b.get("isUnique") or not b.get("mutatorId"):
                raise Unsupported(f"{t} ohne isUnique/mutatorId")
            s = {"kind": None, "group": b["mutatorId"]}
            sets = support_sets(b)
            if sets:
                s["sets"] = sets
            if t == "RangeSupportModel":
                s.update(kind="range", add_frac=b.get("multiplier", 0.0), add=b.get("additive", 0.0))
            elif t == "RateSupportModel":
                if not 0.0 < b["multiplier"] <= 1.0:
                    raise Unsupported(f"RateSupportModel(multiplier {b['multiplier']})")
                s.update(kind="rate", factor=b["multiplier"])
            elif t == "VisibilitySupportModel":
                s["kind"] = "camo"
            elif t == "PierceSupportModel":
                if b["pierce"] != int(b["pierce"]) or b["pierce"] < 0:
                    raise Unsupported(f"PierceSupportModel(pierce {b['pierce']})")
                s.update(kind="pierce", add=int(b["pierce"]))
            elif t == "ProjectileSpeedSupportModel":
                s.update(kind="projectile_speed", add_frac=b["multiplier"])
            elif t == "DamageTypeSupportModel":
                if b.get("immuneBloonProperties"):
                    raise Unsupported("DamageTypeSupportModel mit Immunitäten")
                s["kind"] = "damage_all"
            use(b)
            supports.append(s)
        elif t == "SupportRemoveFilterOutTagModel":
            # schaltet FilterOutTag-Filter ab, die den MIB-Mutator nennen; solche Filter sperrt
            # convert_filters, also hier ohne eigene Wirkung
            if b.get("removeScriptWithSupportMutatorId") != MIB_MUTATOR or b.get("isGlobal"):
                raise Unsupported("SupportRemoveFilterOutTagModel (anderer Mutator)")
            support_sets(b)
            use(b)
        elif t == "DiscountZoneModel":
            if b.get("isGlobal") or b.get("isGlobalRange") or b.get("isBuffFromArea") or b.get("towerBaseIdList") \
                    or b.get("towerBaseIds") or b.get("upgradeId"):
                raise Unsupported("DiscountZoneModel (global/nur bestimmte Türme)")
            discount.append({"fraction": b["discountMultiplier"], "tier_min": int(b["tierMin"]),
                             "tier_cap": int(b["tierCap"]), "stack": b["stackName"],
                             "stack_limit": int(b["stackLimit"]), "affect_self": bool(b.get("affectSelf"))})
            use(b)
        elif t == "AddBehaviorToBloonInZoneModel":
            # Grow Blocker der Village: noch ohne Wirkung (docs/design-bloons-chimps.md, Schritt 2)
            if any(short(x.get("$type", "")) != "GrowBlockModel" for x in b.get("behaviors") or []):
                raise Unsupported("AddBehaviorToBloonInZoneModel (außer GrowBlock)")
            use(b)
    return supports, discount


def is_income(weapon) -> bool:
    """Bananen-Waffe der Farm: Projektil mit CashModel."""
    return bool(behaviors(weapon["projectile"], "CashModel"))


def convert_income(attack, weapon) -> float:
    """Bananen je Runde × Wert (mit bonusMultiplier) -> Einkommen je Runde. Annahme: alles wird
    eingesammelt; Flugbahn, Lebensdauer und Einsammeln haben dann keine Wirkung [KAL]."""
    cash = behaviors(weapon["projectile"], "CashModel")
    per = [b for b in weapon.get("behaviors") or [] if short(b.get("$type", "")) == "EmissionsPerRoundFilterModel"]
    if len(cash) != 1 or len(per) != 1:
        raise Unsupported("Einkommens-Waffe ohne genau ein CashModel/EmissionsPerRoundFilterModel")
    c = cash[0]
    if c["minimum"] != c["maximum"]:
        raise Unsupported("CashModel mit zufälligem Wert")
    if short(weapon["emission"]["$type"]) not in ("SingleEmmisionTowardsTargetModel", "SingleEmissionModel"):
        raise Unsupported("Einkommens-Waffe mit Mehrfach-Emission")
    if c.get("emittedByCashEarnedMultiplier", 1.0) != 1.0:
        raise Unsupported("CashModel(emittedByCashEarnedMultiplier)")
    if len(attack.get("weapons") or []) != 1:
        raise Unsupported("Einkommens-Waffe zusammen mit anderen Waffen")

    def mark(n):
        if isinstance(n, dict):
            if short(n.get("$type", "")) in INCOME_MODELS:
                use(n)
            for k, v in n.items():
                if not k[:1].isupper():
                    mark(v)
        elif isinstance(n, list):
            for v in n:
                mark(v)
    # nur diese Waffe und die Zielposition der Attacke (Bananen landen zufällig um die Farm)
    mark(weapon)
    for n in behaviors(attack, "RandomPositionBasicModel") + [attack.get("targetProvider")]:
        if isinstance(n, dict) and short(n.get("$type", "")) == "RandomPositionBasicModel":
            use(n)
    return per[0]["count"] * c["minimum"] * (1.0 + c.get("bonusMultiplier", 0.0))


def convert_footprint(model) -> tuple:
    """-> (Radius, rechteckig?). Rechtecke (Farm) als Außenkreis: etwas strenger als im Spiel, aber
    nie über Track oder anderen Türmen [KAL]."""
    fp = model["footprint"]
    t = short(fp.get("$type", ""))
    for b in [fp] + behaviors(model, t):
        use(b)
    if any(fp.get(k) for k in ("doesntBlockTowerPlacement", "ignoresPlacementCheck", "ignoresTowerOverlap")):
        raise Unsupported(f"{t} mit Sonderregeln für die Platzierung")
    if t == "RectangleFootprintModel":
        return math.hypot(fp["xWidth"], fp["yWidth"]) / 2.0, True
    if t != "CircleFootprintModel":
        raise Unsupported(f"Footprint {t}")
    return fp["radius"], False


def convert_variant(model, tiers) -> dict:
    _used.clear()  # id() gilt nur, solange das Modell lebt
    unknown = set()
    check_types(model, unknown)
    if unknown:
        raise Unsupported(", ".join(sorted(unknown)))
    attacks, income = [], None
    for a in behaviors(model, "AttackModel"):
        for i, w in enumerate(a.get("weapons") or []):  # ohne Waffen (Village): nur Reichweitenkreis
            if is_income(w):
                if income is not None:
                    raise Unsupported("mehrere Einkommens-Waffen")
                income = {"per_round": convert_income(a, w)}
            else:
                attacks.append(convert_attack(model, a, w, len(attacks)))
    for b in behaviors(model, "CanBuffIndicatorModel"):
        if b.get("buffLocsName") in POTION_INDICATORS:
            use(b)
    supports, discount = convert_supports(model)
    footprint, rect = convert_footprint(model)
    bonus = behaviors(model, "PerRoundCashBonusTowerModel") + behaviors(model, "BonusLivesPerRoundModel")
    extra = behaviors(model, "CollectCashZoneModel") + behaviors(model, "CentralMarketBuffModel")
    if (bonus or extra) and income is None:
        raise Unsupported("Einkommens-Bonus ohne Bananen")
    for b in bonus:
        if short(b["$type"]) == "PerRoundCashBonusTowerModel":
            if b.get("distributeCash") or b.get("cashRoundBonusMultiplier"):
                raise Unsupported("PerRoundCashBonusTowerModel(distributeCash/cashRoundBonusMultiplier)")
            income["per_round"] += b.get("cashPerRound", 0.0)
        else:
            income["lives"] = income.get("lives", 0) + int(b["amount"])
    # CollectCashZone: Einsammeln ohne Wirkung; CentralMarketBuff wirkt auf Merchantmen (nicht im Set) [KAL]
    for b in bonus + extra:
        use(b)
    if not attacks and not supports and not discount and income is None:
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
    v = {"tiers": tiers, "range": model["range"], "footprint": footprint, "attacks": attacks}
    if zones:
        v["zones"] = zones
    if supports:
        v["supports"] = supports
    if discount:
        v["discount"] = discount
    if income is not None:
        v["income"] = income
    if model.get("sellbackModifierAdd"):
        v["sell_bonus"] = model["sellbackModifierAdd"]
    if rect:
        v["_rect"] = True  # nur für das Baubar-Raster (main), wird vor dem Schreiben entfernt
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
                def ok(e):
                    return e["tag"] in known and all(t in known for t in e.get("all", []))
                dropped.update(t for e in v for t in [e["tag"], *e.get("all", [])] if t not in known)
                # die Engine nimmt den ersten passenden Eintrag: die Klasse "Moabs" zuletzt, damit
                # z.B. "Bad" (BAD hat auch den Tag "Moabs") Vorrang hat
                v[:] = sorted((e for e in v if ok(e)), key=lambda e: e["tag"] == "Moabs")
            else:
                prune_tags(v, known, dropped)
    elif isinstance(node, list):
        for v in node:
            prune_tags(v, known, dropped)


def placeholder(model) -> dict:
    """Basisvariante ohne Wirkung für einen Turm, dessen Mechanik fehlt (immer gesperrt): echte Reichweite
    und echter Footprint, damit sich Maske und Kollision nicht ändern, wenn die Mechanik kommt."""
    _used.clear()
    footprint, rect = convert_footprint(model)
    v = {"tiers": [0, 0, 0], "range": model["range"], "footprint": footprint, "attacks": []}
    if rect:
        v["_rect"] = True
    return v


def build_tower(dump: Path, base: str, coverage: list, placeholders: list) -> dict:
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
    if not any(v["tiers"] == [0, 0, 0] for v in variants) and base in MECHANIC_LOCKED:
        try:
            variants.append(placeholder(base_model))
        except Unsupported as e:
            sys.exit(f"{base}: Platzhalter: {e}")
        placeholders.append(TOWER_NAMES[base])
    if not any(v["tiers"] == [0, 0, 0] for v in variants):
        sys.exit(f"{base}: Basisvariante wird nicht unterstützt: {next(c[2] for c in coverage if c[:2] == (base, '000'))}")
    variants.sort(key=lambda v: v["tiers"])
    tower_set = TOWER_SET_NAMES.get(base_model.get("towerSet"))
    if tower_set is None:
        sys.exit(f"{base}: unbekanntes towerSet {base_model.get('towerSet')}")
    return {"name": TOWER_NAMES[base], "game_id": base, "set": tower_set, "cost": int(base_model["cost"]),
            "upgrade_costs": costs, "variants": variants}


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

    coverage, placeholders = [], []
    towers = [build_tower(dump, t, coverage, placeholders) for t in TOWER_SETS[args.towers]]
    # Baubar-Raster mit dem größten runden Footprint: mit dem Außenkreis der Farm würde es für alle
    # Türme viel kleiner (die Maske je Turm prüft ohnehin den eigenen Footprint)
    footprint = max(v["footprint"] for t in towers for v in t["variants"] if not v.get("_rect"))
    for t in towers:
        for v in t["variants"]:
            v.pop("_rect", None)
    selling, mod_locked = match_rules(dump, preset["match"], args.map)
    locked = [TOWER_NAMES[b] for b in TOWER_SETS[args.towers] if b in mod_locked or b in MECHANIC_LOCKED]
    last = preset["end_round"]
    rounds = build_rounds(dump, args.rounds, last)
    needed = {g["bloon"] for r in rounds for g in r}
    speed = bloon_speed_factor(dump, preset["match"], args.map)
    bloons = build_bloons(dump, needed, speed)
    dropped: set = set()
    prune_tags(towers, {t for b in bloons for t in b["tags"]}, dropped)
    # Trank-Ausschlüsse nur auf Türme des Sets (game_v1 lehnt unbekannte Namen ab)
    names = {t["name"] for t in towers}
    for a in (a for t in towers for v in t["variants"] for a in v["attacks"]):
        if a["delivery"]["kind"] == "potion":
            a["delivery"]["effect"]["exclude"] = [n for n in a["delivery"]["effect"]["exclude"] if n in names]
    data = {
        "format": "game_v1",
        "name": f"{args.map}-{args.preset}-{dump.name}",
        # Herkunft für die Mod (Replay-Player); Modus/Schwierigkeit aus dem Partie-Namen
        # "<Spieltyp>_<Modus>_<Schwierigkeit>"
        "meta": {"map": args.map, "mode": preset["match"].split("_")[1],
                 "difficulty": preset["match"].split("_")[2], "game_version": dump.name},
        "rules": {"tick_rate": 60.0, "cd_eps": 1e-9, "max_round_seconds": 900.0, "child_spacing": 0.0,
                  "sell_ratio": SELL_RATIO, "cost_rounding": 5, "selling": selling,
                  **{k: v for k, v in preset.items() if k != "match"}, **({"locked_towers": locked} if locked else {})},
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
             "- Auren, Rabatte und Einkommen (Village, Farm) stehen in den Daten, wirken in der Engine aber noch "
             "nicht (docs/design-towers-step2.md, Schritt 1).",
             "- Regrow ohne Grow Blocker der Village; Bloons ab Runde 81 ohne die stärkere Skalierung des Spiels "
             "(docs/design-bloons-chimps.md).",
             f"- Gesperrte Türme (rules.locked_towers): {', '.join(locked) or '-'}.",
             *([f"- Platzhalter ohne Wirkung (Mechanik fehlt, Basisvariante geht noch nicht): "
                f"{', '.join(placeholders)}."] if placeholders else []),
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
