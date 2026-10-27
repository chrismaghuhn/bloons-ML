"""data/raw/towers/*.json -> data/base/towers/*.json (+ data/base/report.md)

Die Rohdaten stammen von vier Agents mit unterschiedlichen Konventionen:
  - Absolutwerte pro Tier            {"pierce": 3}
  - Deltas als String                {"pierce": "+1"}, {"cooldown": "x0.8"}, {"range": "+10%"}
  - Deltas mit Vorher/Nachher        {"cooldown": "x0.66 (0.2 -> 0.132)"}
  - Angriff per Präfix adressiert    {"Attack/Projectile.damage": "+2"}
Hier wird pro Pfad Tier für Tier ein vollständiger Zustand mit Absolutwerten berechnet.
Alles, was (noch) nicht modelliert ist, landet unverändert in `effects` und im Report.

    python tools/normalize.py
"""
from __future__ import annotations

import copy
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / "data" / "raw" / "towers"
OUT = ROOT / "data" / "base" / "towers"
REPORT = ROOT / "data" / "base" / "report.md"
SCHEMA = ROOT / "data" / "schema" / "tower.schema.json"

PATHS = ("top", "middle", "bottom")
ATTACK_FIELDS = ("damage", "pierce", "cooldown", "damage_type", "projectiles", "blast_radius")
NOTE_FIELDS = ("notes", "notes_alternation")

# Türme, deren Wiki-`range` nicht die Zielreichweite ist (s. docs/engine-design.md, Reach)
REACH = {
    "sniper_monkey": "global",
    "mortar_monkey": "reticle",
    "dartling_gunner": "reticle",
    "monkey_ace": "flying",
    "heli_pilot": "follow",
}
DAMAGE_TYPES = {"explosion": "explosive"}
BLOON_CLASSES = {
    "moab": "moab", "moab-class": "moab", "moabs": "moab", "ceramic": "ceramic", "ceramics": "ceramic",
    "lead": "lead", "fortified": "fortified", "camo": "camo", "frozen": "frozen",
    "stunned": "stunned", "boss": "boss", "bosses": "boss", "purple": "purple",
}

NUM = r"-?\d+(?:\.\d+)?"


class Log:
    def __init__(self):
        self.unparsed: list[str] = []
        self.missing: list[str] = []
        self.effects = Counter()
        self.damage_types = Counter()


def slug(s: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", s.lower()).strip("_") or "attack"


def apply_value(old, v, where: str, log: Log):
    """Rohwert v auf alten Wert anwenden -> neuer Absolutwert."""
    if v is None:
        log.missing.append(f"{where}: Wert im Wiki unbekannt (null)")
        return None
    if isinstance(v, bool) or isinstance(v, (int, float)):
        return v
    s = str(v).strip()
    m = re.search(rf"->\s*({NUM})", s)  # "x0.66 (0.2 -> 0.132)": Endwert nehmen
    if m:
        return float(m.group(1))
    m = re.fullmatch(rf"~?([+-]{NUM})\s*%.*", s)
    if m and isinstance(old, (int, float)):
        return old * (1 + float(m.group(1)) / 100)
    m = re.fullmatch(rf"([+-]{NUM})(?:\s.*)?", s)
    if m and isinstance(old, (int, float)):
        return old + float(m.group(1))
    m = re.fullmatch(rf"[x×]\s*({NUM})(?:\s.*)?", s)
    if m and isinstance(old, (int, float)):
        return old * float(m.group(1))
    if re.fullmatch(NUM, s):
        return float(s)
    log.unparsed.append(f"{where}: {s!r} (alter Wert {old!r})")
    return old


def clean_number(x):
    if isinstance(x, float):
        x = round(x, 6)
        if x.is_integer():
            return int(x)
    return x


def parse_notes(atk: dict):
    """Zusatzwerte aus dem Notiz-Text ziehen (die Notizen beschreiben jeweils den vollen Stand)."""
    text = atk.get("notes") or ""
    if not isinstance(text, str):
        text = json.dumps(text, ensure_ascii=False)
        atk["notes"] = text
    vs = {}  # erster Treffer gewinnt: Hauptprojektil steht vor Sub-Projektilen (Splitter etc.)
    for cls, val in re.findall(rf"Damage to ([A-Za-z][A-Za-z\- ]*?): \+({NUM})", text):
        vs.setdefault(cls.strip().lower(), float(val))
    for val, cls in re.findall(rf"\+({NUM}) damage vs ([A-Za-z][A-Za-z\-]*)", text):
        vs.setdefault(cls.strip().lower(), float(val))
    if vs or "damage_vs" not in atk:
        atk["damage_vs"] = {BLOON_CLASSES.get(k, k): clean_number(v) for k, v in vs.items()}
    m = re.search(rf"Speed: ({NUM}) units/s", text) or re.search(rf"\bspeed=({NUM})", text)
    if m:
        atk["projectile_speed"] = clean_number(float(m.group(1)))
    m = re.search(rf"Lifespan: ({NUM})s", text) or re.search(rf"\blifespan=({NUM})s", text)
    if m:
        atk["lifespan"] = clean_number(float(m.group(1)))
    if re.search(r"cannot (target|hit) camo", text, re.I):
        atk["camo"] = False
    elif re.search(r"\bcan (target|hit|see) camo", text, re.I):
        atk["camo"] = True


def make_attack(raw: dict, log: Log, where: str) -> dict:
    a = {
        "tag": slug(raw.get("name") or "attack"),
        "name": raw.get("name") or "attack",
        **{f: raw.get(f) for f in ATTACK_FIELDS},
        "projectile_speed": None,
        "lifespan": None,
        "camo": None,
        "notes": raw.get("notes") or "",
    }
    dt = a["damage_type"]
    if isinstance(dt, str):
        a["damage_type"] = DAMAGE_TYPES.get(dt.lower(), dt.lower())
    parse_notes(a)
    for k, v in raw.items():
        if k not in a and k != "name":
            log.effects[f"attack.{k}"] += 1
            a["notes"] = (a["notes"] + f"; {k}={json.dumps(v, ensure_ascii=False)}").lstrip("; ")
    return a


def find_attack(state: dict, name: str):
    for a in state["attacks"]:
        if a["name"] == name or a["tag"] == slug(name):
            return a
    return None


def set_attack_field(atk: dict, field: str, v, where: str, log: Log):
    if field in NOTE_FIELDS:
        atk["notes"] = v if isinstance(v, str) else json.dumps(v, ensure_ascii=False)
        parse_notes(atk)
    elif field == "damage_type":
        atk["damage_type"] = DAMAGE_TYPES.get(str(v).lower(), str(v).lower()) if v else None
    else:
        atk[field] = apply_value(atk.get(field), v, where, log)


def apply_changes(state: dict, changes: dict, tower: str, where: str, log: Log) -> dict:
    """Wendet die Änderungen eines Tiers auf `state` an, gibt die nicht modellierten zurück."""
    effects = {}
    main = lambda: state["attacks"][0] if state["attacks"] else None

    for key, v in changes.items():
        w = f"{where} {key}"
        if key == "range":
            state["range"] = apply_value(state["range"], v, w, log)
        elif key == "camo":
            state["camo"] = bool(v)
        elif key in ATTACK_FIELDS or key in NOTE_FIELDS:
            if main() is None:
                log.unparsed.append(f"{w}: kein Hauptangriff vorhanden")
                effects[key] = v
            else:
                set_attack_field(main(), key, v, w, log)
        elif key in ("new_attack", "new_attacks", "replace_attack"):
            for raw in v if isinstance(v, list) else [v]:
                new = make_attack(raw, log, w)
                old = find_attack(state, new["name"])
                if old is not None:
                    state["attacks"][state["attacks"].index(old)] = new
                elif key == "replace_attack" and state["attacks"]:
                    state["attacks"][0] = new
                else:
                    state["attacks"].append(new)
        elif key == "removed_attacks":
            for name in v:
                a = find_attack(state, name)
                if a is None:
                    log.unparsed.append(f"{w}: zu entfernender Angriff {name!r} nicht gefunden")
                else:
                    state["attacks"].remove(a)
        elif key == "modified_attacks":
            for name, fields in v.items():
                a = find_attack(state, name)
                if a is None:
                    log.unparsed.append(f"{w}: Angriff {name!r} nicht gefunden")
                    effects[key] = v
                    continue
                for f, fv in fields.items():
                    if f in ATTACK_FIELDS or f in NOTE_FIELDS:
                        set_attack_field(a, f, fv, f"{w}.{name}.{f}", log)
                    else:
                        effects[f"{name}.{f}"] = fv
        elif "." in key and not key.startswith(("buff:", "zone:")):
            name, field = key.rsplit(".", 1)
            a = find_attack(state, name)
            if a is not None and (field in ATTACK_FIELDS or field in NOTE_FIELDS):
                set_attack_field(a, field, v, w, log)
            else:
                effects[key] = v  # z.B. Ability.cooldown, Angriffe aus Fähigkeiten
        else:
            effects[key] = v

    for k in effects:
        log.effects[k.split(":")[0] + (":*" if ":" in k else "")] += 1
    return effects


def footprint(raw: dict, log: Log):
    r = raw.get("footprint_radius")
    if isinstance(r, (int, float)):
        return {"radius": r}
    log.missing.append(f"{raw['name']}: footprint unbekannt (null)")
    return None


def normalize(raw: dict, tid: str, log: Log) -> dict:
    base = {
        "range": raw.get("range"),
        "camo": bool(raw.get("camo")),
        "reach": REACH.get(tid, "radius"),
        "attacks": [make_attack(a, log, f"{tid} base") for a in raw["attacks"]],
    }
    paths = {}
    for p in PATHS:
        state, tiers = copy.deepcopy(base), []
        for up in sorted(raw["upgrades"][p], key=lambda u: u["tier"]):
            where = f"{tid} {p}{up['tier']} ({up['name']})"
            effects = apply_changes(state, up.get("changes") or {}, tid, where, log)
            if "footprint" in effects:
                m = re.search(rf"rectangle ({NUM})x({NUM})", str(effects["footprint"]))
                if m:
                    effects["footprint_rect"] = [float(m.group(1)), float(m.group(2))]
            tier = {"tier": up["tier"], "name": up["name"], "cost": up.get("cost"),
                    "description": up.get("description") or "",
                    "state": copy.deepcopy(state), "effects": effects}
            if tier["cost"] is None:
                log.missing.append(f"{where}: Kosten unbekannt")
            tiers.append(tier)
        paths[p] = tiers

    for st in [base] + [t["state"] for p in PATHS for t in paths[p]]:
        st["range"] = clean_number(st["range"])
        for a in st["attacks"]:
            for f in ATTACK_FIELDS + ("projectile_speed", "lifespan"):
                if isinstance(a.get(f), float):
                    a[f] = clean_number(a[f])
            if a["damage_type"]:
                log.damage_types[a["damage_type"]] += 1

    return {
        "id": tid,
        "name": raw["name"],
        "category": raw["category"],
        "cost": raw["cost"],
        "placement": raw.get("placement") or "land",
        "footprint": footprint(raw, log),
        "game_version": raw.get("game_version"),
        "sources": raw.get("source_urls") or [],
        "abilities": raw.get("abilities") or [],
        "base": base,
        "paths": paths,
    }


def validate(docs: dict[str, dict]) -> list[str]:
    try:
        import jsonschema
    except ImportError:
        return ["jsonschema nicht installiert, Schema-Prüfung übersprungen"]
    schema = json.loads(SCHEMA.read_text())
    v = jsonschema.Draft202012Validator(schema)
    return [f"{tid}: {'/'.join(map(str, e.path))}: {e.message}" for tid, d in docs.items() for e in v.iter_errors(d)]


def write_report(docs, logs: dict[str, Log], schema_errors):
    total = Log()
    lines = ["# Normalisierungs-Report", "",
             "Erzeugt von `tools/normalize.py`. Nicht von Hand bearbeiten.", "",
             "| Turm | Version | Angriffe (Basis) | unparsed | fehlend | nicht modelliert |",
             "|---|---|---|---|---|---|"]
    for tid, d in sorted(docs.items()):
        lg = logs[tid]
        total.effects.update(lg.effects)
        total.damage_types.update(lg.damage_types)
        lines.append(f"| {tid} | {d['game_version']} | {len(d['base']['attacks'])} | "
                     f"{len(lg.unparsed)} | {len(lg.missing)} | {sum(lg.effects.values())} |")
    lines += ["", "## Schema", ""]
    lines += [f"- {e}" for e in schema_errors] or ["- alle Dateien gültig"]
    lines += ["", "## Schadenstypen", ""] + [f"- `{k}`: {c}" for k, c in total.damage_types.most_common()]
    lines += ["", "## Nicht modellierte Schlüssel (stehen in `effects`)", ""]
    lines += [f"- `{k}`: {c}" for k, c in total.effects.most_common()]
    for title, attr in (("Nicht interpretierbare Werte", "unparsed"), ("Fehlende Werte (null im Wiki)", "missing")):
        lines += ["", f"## {title}", ""]
        for tid in sorted(logs):
            lines += [f"- {x}" for x in getattr(logs[tid], attr)]
    REPORT.write_text("\n".join(lines) + "\n")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    docs, logs = {}, defaultdict(Log)
    for f in sorted(RAW.glob("*.json")):
        tid = f.stem
        docs[tid] = normalize(json.loads(f.read_text()), tid, logs[tid])
        (OUT / f.name).write_text(json.dumps(docs[tid], indent=2, ensure_ascii=False) + "\n")
    errors = validate(docs)
    write_report(docs, logs, errors)
    unparsed = sum(len(l.unparsed) for l in logs.values())
    print(f"{len(docs)} Türme -> {OUT.relative_to(ROOT)}  |  unparsed: {unparsed}  |  "
          f"Schema-Fehler: {len(errors)}  |  Report: {REPORT.relative_to(ROOT)}")
    return 1 if errors and "nicht installiert" not in errors[0] else 0


if __name__ == "__main__":
    sys.exit(main())
