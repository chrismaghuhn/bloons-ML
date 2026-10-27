"""data/raw/towers/*.json -> data/base/towers/*.json (+ data/base/report.md)

Die Rohdaten stammen von vier Agents mit unterschiedlichen Konventionen:
  - Absolutwerte pro Tier            {"pierce": 3}
  - Deltas als String                {"pierce": "+1"}, {"cooldown": "x0.8"}, {"range": "+10%"}
  - Deltas mit Vorher/Nachher        {"cooldown": "x0.66 (0.2 -> 0.132)"}
  - Angriff per Präfix adressiert    {"Attack/Projectile.damage": "+2"}

Pro Tier entstehen zwei Dinge:
  - `ops`:   die Änderungen dieses Tiers als set/add/mul mit Ziel (Quelle für die Modifier
             der Engine, nötig für Crosspaths)
  - `state`: der vollständige Zustand mit Absolutwerten (geprüfter Snapshot)
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
DERIVED_FIELDS = ("range", "projectile_speed", "lifespan", "damage_vs", "camo")  # aus den Notizen

# Türme, deren Wiki-`range` nicht die Zielreichweite ist (s. docs/engine-design.md, Reach)
REACH = {
    "sniper_monkey": "global",
    "mortar_monkey": "reticle",
    "dartling_gunner": "reticle",
    "monkey_ace": "flying",
    "heli_pilot": "follow",
}
DAMAGE_TYPES = {"explosion": "explosive"}
KNOWN_DAMAGE_TYPES = {"sharp", "normal", "explosive", "energy", "plasma", "shatter", "acid",
                      "fire", "frigid", "cold", "glacier"}

# Festes Vokabular für damage_vs: Bloon-Klassen und Status. Zusammengesetzte Bedingungen
# (beides muss zutreffen) werden alphabetisch mit "+" verbunden, z.B. "fortified+moab".
BLOON_CLASSES = {
    "moab": "moab", "moabs": "moab", "blimp": "moab", "blimps": "moab",
    "bfb": "bfb", "zomg": "zomg", "ddt": "ddt", "ddts": "ddt", "bad": "bad",
    "ceramic": "ceramic", "ceramics": "ceramic", "ceramicexceptcas": "ceramic",
    "lead": "lead", "leads": "lead", "fortified": "fortified", "camo": "camo",
    "purple": "purple", "boss": "boss", "bosses": "boss",
}
STATUS_CLASSES = {
    "stunned": "stunned", "frozen": "frozen", "stickied": "stickied", "glued": "glued",
    "shocked": "shocked", "burning": "burning",
}
VOCAB = {**BLOON_CLASSES, **STATUS_CLASSES}
CLASS_NAMES = sorted(set(VOCAB.values()))
FILLER = {"class", "bloon", "bloons", "damage", "type", "types", "only", "and", "or", "vs", "to"}

NUM = r"-?\d+(?:\.\d+)?"


class Log:
    def __init__(self):
        self.unparsed: list[str] = []
        self.missing: list[str] = []
        self.notes: list[str] = []          # Hinweise ohne Fehlercharakter
        self.camo_exceptions: list[str] = []
        self.effects = Counter()
        self.damage_types = Counter()


def slug(s: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", s.lower()).strip("_") or "attack"


def clean_number(x):
    """Rundungsreste aus Multiplikatoren entfernen (2.4999 -> 2.5)."""
    if isinstance(x, float):
        x = float(f"{x:.4g}") if abs(x) < 1000 else round(x, 2)
        if x.is_integer():
            return int(x)
    return x


def apply_value(old, v, where: str, log: Log):
    """Rohwert auf alten Wert anwenden -> (neuer Absolutwert, op, operand)."""
    if v is None:
        log.missing.append(f"{where}: Wert im Wiki unbekannt (null)")
        return None, "set", None
    if isinstance(v, bool) or isinstance(v, (int, float)):
        return v, "set", v
    s = str(v).strip()
    num_old = isinstance(old, (int, float)) and not isinstance(old, bool)

    m = re.fullmatch(rf"[x×]\s*({NUM})\s*\(\s*({NUM})\s*->\s*({NUM})\s*\)", s)
    if m:  # "x0.66 (0.2 -> 0.132)": Endwert nehmen, Startwert prüfen
        start, end = float(m.group(2)), float(m.group(3))
        if num_old and abs(start - old) > 0.01 * max(abs(old), 1e-9):
            log.unparsed.append(f"{where}: {s!r} Startwert passt nicht zum alten Wert {old!r}")
        return end, "mul", float(m.group(1))
    m = re.fullmatch(rf"({NUM})\s*->\s*({NUM})", s)
    if m:
        return float(m.group(2)), "set", float(m.group(2))

    for pat, op in ((rf"~?([+-]{NUM})\s*%(.*)", "pct"), (rf"([+-]{NUM})(.*)", "add"),
                    (rf"[x×]\s*({NUM})(.*)", "mul")):
        m = re.fullmatch(pat, s)
        if not m:
            continue
        if not num_old:
            log.unparsed.append(f"{where}: Delta {s!r} ohne alten Zahlenwert ({old!r})")
            return old, None, None
        if m.group(2).strip():
            log.unparsed.append(f"{where}: {s!r} hat Zusatztext, nur die Zahl wurde übernommen")
        x = float(m.group(1))
        if op == "pct":
            return old * (1 + x / 100), "mul", 1 + x / 100
        if op == "add":
            return old + x, "add", x
        return old * x, "mul", x

    if re.fullmatch(NUM, s):
        return float(s), "set", float(s)
    log.unparsed.append(f"{where}: {s!r} (alter Wert {old!r})")
    return old, None, None


# ---------------------------------------------------------------- Notizen
def strip_nested(text: str) -> str:
    """Entfernt alle {...}-Blöcke (inkl. Name davor) aus einem Text."""
    out, depth = [], 0
    for ch in text:
        if ch == "{":
            depth += 1
            while out and out[-1] not in ";":
                out.pop()  # Blockname davor gehört zum Block
        elif ch == "}":
            depth = max(depth - 1, 0)
        elif depth == 0:
            out.append(ch)
    return "".join(out)


def top_blocks(text: str) -> list[tuple[str, str]]:
    """Blöcke auf oberster Ebene: [(name, inhalt ohne Unterblöcke)]."""
    blocks, depth, start, name = [], 0, 0, ""
    for i, ch in enumerate(text):
        if ch == "{":
            if depth == 0:
                name = text[:i].rsplit(";", 1)[-1].strip()
                start = i + 1
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                blocks.append((name, strip_nested(text[start:i])))
    return blocks


def sections(text: str, has_blast: bool) -> tuple[str, str, str]:
    """(Turm-/Angriffsebene, Projektil, Schadensquelle) aus einer Notiz.

    Primary-Notizen enthalten verschachtelte Wiki-Blöcke ("Projectile {...}; Explosion {...}").
    Bei Splash-Angriffen kommt der Schaden aus der Explosion. Unterblöcke (Damage over time,
    Glue, Splitter ...) zählen nicht zum Angriff selbst.
    Andere Agents schreiben flachen Text; dort werden "effect '...': ..."-Teile ignoriert.
    """
    if "{" in text:
        blocks = top_blocks(text)
        outer = strip_nested(text)
        proj = next((c for n, c in blocks if "projectile" in n.lower()), blocks[0][1] if blocks else "")
        expl = next((c for n, c in blocks if "explosion" in n.lower()), None)
        dmg = expl if (has_blast and expl is not None) else proj
        return outer, proj, dmg
    flat = re.sub(r"effect '[^']*'[^;]*", "", text)
    return flat, flat, flat


def parse_classes(phrase: str):
    """'fortified MOAB-class' -> ['fortified+moab'], 'Ceramic or Fortified' -> ['ceramic', 'fortified']."""
    phrase = re.split(r"\bexcept\b|\(", phrase, flags=re.I)[0]
    out = []
    for tok in re.split(r"\s*(?:/|,|\bor\b|\band\b)\s*", phrase, flags=re.I):
        words = re.findall(r"[a-z]+", tok.lower().replace("-class", " class"))
        if not words:
            continue
        if "non" in words:
            return None
        mapped, unknown = [], []
        for w in words:
            if w in VOCAB:
                mapped.append(VOCAB[w])
            elif w not in FILLER:
                unknown.append(w)
        if not mapped:
            if out:
                break  # Ende der Klassen-Aufzählung
            return None
        if unknown:
            return None
        out.append("+".join(sorted(set(mapped))))
    return out or None


def parse_damage_vs(text: str, where: str, log: Log) -> dict:
    vs: dict[str, float] = {}

    def put(classes, val):
        for c in classes:
            vs.setdefault(c, float(val))  # erster Treffer gewinnt

    for phrase, val in re.findall(rf"Damage to ([^:;{{}}]{{1,40}}): \+({NUM})", text):
        cls = parse_classes(phrase)
        if cls:
            put(cls, val)
        else:
            log.unparsed.append(f"{where}: Bonus-Klasse {phrase!r} unbekannt")
    for val, phrase in re.findall(rf"\+({NUM})(?![\d.%])\s+([^;,(){{}}]{{1,45}})", text):
        p = re.sub(r"^(?:damage\s+)?(?:vs\.?|to|against)\s+", "", phrase.strip(), flags=re.I)
        p = re.sub(r"^damage\s+", "", p, flags=re.I)
        first = re.findall(r"[a-z]+", p.lower())[:1]
        if not first or first[0] not in VOCAB:
            if any(w in VOCAB for w in re.findall(r"[a-z]+", p.lower())) and re.search(r"damage", phrase, re.I):
                log.unparsed.append(f"{where}: möglicher Bonus nicht erkannt: '+{val} {phrase.strip()}'")
            continue
        cls = parse_classes(p)
        if cls:
            put(cls, val)
        else:
            log.unparsed.append(f"{where}: Bonus '+{val} {phrase.strip()}' nicht eindeutig")
    for w in re.findall(r"\bshocked(\d+)", text, re.I):
        log.notes.append(f"{where}: Shocked-Stufe {w} zu 'shocked' zusammengefasst")
    return {k: clean_number(v) for k, v in vs.items()}


def find_num(patterns, text):
    for p in patterns:
        m = re.search(p, text, re.I)
        if m:
            return clean_number(float(m.group(1)))
    return None


def parse_notes(atk: dict, where: str, log: Log):
    """Leitet Zusatzwerte aus den Notizen ab. Die Notizen beschreiben jeweils den vollen Stand,
    deshalb wird jedes Feld neu gesetzt (auch auf None), statt alte Werte stehen zu lassen."""
    text = atk.get("notes") or ""
    if not isinstance(text, str):
        text = json.dumps(text, ensure_ascii=False)
        atk["notes"] = text
    outer, proj, dmg = sections(text, bool(atk.get("blast_radius")))
    atk["damage_vs"] = parse_damage_vs(dmg, where, log)
    not_speed = r"(?<!attack )(?<!movement )(?<!rotation )(?<!flight )(?<!fly )"
    atk["projectile_speed"] = find_num(
        [rf"\bSpeed: ({NUM}) units/s", rf"{not_speed}\bspeed\s*[:=]?\s*({NUM})(?!\s*%)"], proj)
    atk["lifespan"] = find_num([rf"\bLifespan: ({NUM})s", rf"\blifespan\s*[:=]?\s*({NUM})s?\b"], proj)
    atk["range"] = find_num([rf"Targeting range: ({NUM})", rf"\b(?:attack|targeting) range\s*[:=]?\s*({NUM})"], outer)
    if re.search(r"cannot (?:target|hit|see|detect) camo", outer + proj, re.I):
        atk["camo"] = False
    elif re.search(r"\bcan (?:target|hit|see|detect) camo", outer + proj, re.I):
        atk["camo"] = True
    else:
        atk["camo"] = None


def make_attack(raw: dict, log: Log, where: str) -> dict:
    a = {
        "tag": slug(raw.get("name") or "attack"),
        "name": raw.get("name") or "attack",
        **{f: raw.get(f) for f in ATTACK_FIELDS},
        "range": None, "projectile_speed": None, "lifespan": None,
        "damage_vs": {}, "camo": None,
        "notes": raw.get("notes") or "",
    }
    if isinstance(a["damage_type"], str):
        a["damage_type"] = DAMAGE_TYPES.get(a["damage_type"].lower(), a["damage_type"].lower())
    parse_notes(a, where, log)
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


# ---------------------------------------------------------------- Tiers
class TierCtx:
    def __init__(self, where, log):
        self.where, self.log = where, log
        self.ops: list[dict] = []
        self.effects: dict = {}
        self.notes_updated: set[str] = set()

    def op(self, target, stat, op, value):
        if op is not None:
            self.ops.append({"target": target, "stat": stat, "op": op, "value": clean_number(value)})


def set_attack_field(atk: dict, field: str, v, ctx: TierCtx, w: str):
    if field == "notes":
        atk["notes"] = v if isinstance(v, str) else json.dumps(v, ensure_ascii=False)
        parse_notes(atk, w, ctx.log)
        ctx.notes_updated.add(atk["tag"])
    elif field == "notes_alternation":  # Zusatzinfo, ersetzt die Notizen nicht
        atk["notes"] = f"{atk['notes']}; alternation: {v}".lstrip("; ")
    elif field == "damage_type":
        atk["damage_type"] = DAMAGE_TYPES.get(str(v).lower(), str(v).lower()) if v else None
        ctx.op(atk["tag"], "damage_type", "set", atk["damage_type"])
    else:
        atk[field], op, operand = apply_value(atk.get(field), v, w, ctx.log)
        ctx.op(atk["tag"], field, op, operand)


def add_attack(state, raw, ctx: TierCtx, replace_main: bool):
    new = make_attack(raw, ctx.log, ctx.where)
    old = find_attack(state, new["name"])
    if old is not None:
        state["attacks"][state["attacks"].index(old)] = new
        ctx.ops.append({"target": old["tag"], "stat": "attack", "op": "remove", "value": None})
    elif replace_main and state["attacks"]:
        ctx.ops.append({"target": state["attacks"][0]["tag"], "stat": "attack", "op": "remove", "value": None})
        state["attacks"][0] = new
    else:
        state["attacks"].append(new)
    ctx.ops.append({"target": new["tag"], "stat": "attack", "op": "add", "value": new["name"]})
    ctx.notes_updated.add(new["tag"])


def snapshot_derived(state: dict) -> dict:
    return {a["tag"]: {f: copy.deepcopy(a.get(f)) for f in DERIVED_FIELDS} for a in state["attacks"]}


def derived_ops(state: dict, before: dict, ctx: TierCtx):
    """Aus den Notizen abgeleitete Werte, die sich geändert haben, als set-ops festhalten.
    Läuft nach resolve_camo/finalize, damit nur echte Änderungen auftauchen."""
    for a in state["attacks"]:
        old = before.get(a["tag"])
        if old is None:
            continue  # neuer Angriff, steckt komplett im add-op
        for f in DERIVED_FIELDS:
            if a.get(f) != old[f]:
                ctx.op(a["tag"], f, "set", a.get(f))


def apply_changes(state: dict, changes: dict, ctx: TierCtx):
    main = lambda: state["attacks"][0] if state["attacks"] else None

    for key, v in changes.items():
        w = f"{ctx.where} {key}"
        if key == "range":
            state["range"], op, operand = apply_value(state["range"], v, w, ctx.log)
            ctx.op("tower", "range", op, operand)
        elif key == "camo":
            state["camo"] = bool(v)
            ctx.op("tower", "camo", "set", bool(v))
        elif key in ATTACK_FIELDS or key in ("notes", "notes_alternation"):
            if main() is None:
                ctx.log.unparsed.append(f"{w}: kein Hauptangriff vorhanden")
                ctx.effects[key] = v
            else:
                set_attack_field(main(), key, v, ctx, w)
        elif key in ("new_attack", "new_attacks", "replace_attack"):
            for raw in v if isinstance(v, list) else [v]:
                add_attack(state, raw, ctx, replace_main=(key == "replace_attack"))
        elif key == "removed_attacks":
            for name in v:
                a = find_attack(state, name)
                if a is None:
                    ctx.log.unparsed.append(f"{w}: zu entfernender Angriff {name!r} nicht gefunden")
                else:
                    state["attacks"].remove(a)
                    ctx.ops.append({"target": a["tag"], "stat": "attack", "op": "remove", "value": None})
        elif key == "modified_attacks":
            for name, fields in v.items():
                a = find_attack(state, name)
                if a is None:
                    ctx.log.unparsed.append(f"{w}: Angriff {name!r} nicht gefunden")
                    ctx.effects[f"{name}"] = fields
                    continue
                for f, fv in fields.items():
                    if f in ATTACK_FIELDS or f in ("notes", "notes_alternation"):
                        set_attack_field(a, f, fv, ctx, f"{w}.{name}.{f}")
                    else:
                        ctx.effects[f"{name}.{f}"] = fv
        elif "." in key and not key.startswith(("buff:", "zone:")):
            name, field = key.rsplit(".", 1)
            a = find_attack(state, name)
            if a is not None and (field in ATTACK_FIELDS or field in ("notes", "notes_alternation")):
                set_attack_field(a, field, v, ctx, w)
            else:
                if a is None and "ability" not in name.lower():
                    ctx.log.unparsed.append(f"{w}: Angriff {name!r} nicht gefunden")
                ctx.effects[key] = v
        else:
            ctx.effects[key] = v

    for k in ctx.effects:
        ctx.log.effects[k.split(":")[0] + (":*" if ":" in k else "")] += 1


def resolve_camo(state: dict, ctx: TierCtx):
    """Camo gehört zum Turm. Auf Angriffs-Ebene bleibt nur eine explizite Ausnahme stehen:
    camo=False, obwohl der Turm Camo hat, und die Notizen dieses Tiers sagen das ausdrücklich."""
    for a in state["attacks"]:
        if a["camo"] is False and state["camo"]:
            if a["tag"] in ctx.notes_updated:
                ctx.log.camo_exceptions.append(
                    f"{ctx.where}: Turm hat Camo, Angriff `{a['tag']}` laut Notizen nicht (Patch prüfen)")
            else:
                a["camo"] = None
        elif a["camo"] is True and state["camo"]:
            a["camo"] = None  # redundant
        elif a["camo"] is False and not state["camo"]:
            a["camo"] = None  # entspricht dem Turm


def finalize(state: dict, log: Log):
    state["range"] = clean_number(state["range"])
    for a in state["attacks"]:
        for f in ATTACK_FIELDS + ("range", "projectile_speed", "lifespan"):
            if isinstance(a.get(f), float):
                a[f] = clean_number(a[f])
        if a["range"] is not None and a["range"] == state["range"]:
            a["range"] = None  # nur abweichende Reichweiten speichern (RangeOverride)
        if a["damage_type"]:
            log.damage_types[a["damage_type"]] += 1
            if a["damage_type"] not in KNOWN_DAMAGE_TYPES:
                log.unparsed.append(f"Schadenstyp {a['damage_type']!r} unbekannt ({a['tag']})")


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
    base_ctx = TierCtx(f"{tid} base", log)
    base_ctx.notes_updated = {a["tag"] for a in base["attacks"]}
    resolve_camo(base, base_ctx)
    finalize(base, log)

    paths = {}
    for p in PATHS:
        state, tiers = copy.deepcopy(base), []
        for up in sorted(raw["upgrades"][p], key=lambda u: u["tier"]):
            ctx = TierCtx(f"{tid} {p}{up['tier']} ({up['name']})", log)
            before = snapshot_derived(state)
            apply_changes(state, up.get("changes") or {}, ctx)
            resolve_camo(state, ctx)
            finalize(state, log)
            derived_ops(state, before, ctx)
            if "footprint" in ctx.effects:
                m = re.search(rf"rectangle ({NUM})x({NUM})", str(ctx.effects["footprint"]))
                if m:
                    ctx.effects["footprint_rect"] = [float(m.group(1)), float(m.group(2))]
            if up.get("cost") is None:
                log.missing.append(f"{ctx.where}: Kosten unbekannt")
            tiers.append({"tier": up["tier"], "name": up["name"], "cost": up.get("cost"),
                          "description": up.get("description") or "",
                          "ops": ctx.ops, "state": copy.deepcopy(state), "effects": ctx.effects})
        paths[p] = tiers

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


# ---------------------------------------------------------------- Ausgabe
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
             "| Turm | Version | Angriffe (Basis) | unparsed | fehlend | Camo-Ausnahmen | nicht modelliert |",
             "|---|---|---|---|---|---|---|"]
    for tid, d in sorted(docs.items()):
        lg = logs[tid]
        total.effects.update(lg.effects)
        total.damage_types.update(lg.damage_types)
        lines.append(f"| {tid} | {d['game_version']} | {len(d['base']['attacks'])} | {len(lg.unparsed)} | "
                     f"{len(lg.missing)} | {len(lg.camo_exceptions)} | {sum(lg.effects.values())} |")
    lines += ["", "## Schema", ""]
    lines += [f"- {e}" for e in schema_errors] or ["- alle Dateien gültig"]
    lines += ["", "## Schadenstypen", ""] + [f"- `{k}`: {c}" for k, c in total.damage_types.most_common()]
    lines += ["", "## Nicht modellierte Schlüssel (stehen in `effects`)", ""]
    lines += [f"- `{k}`: {c}" for k, c in total.effects.most_common()]
    for title, attr in (("Nicht interpretierbare Werte", "unparsed"),
                        ("Camo-Ausnahmen (Turm hat Camo, Angriff laut Notizen nicht)", "camo_exceptions"),
                        ("Fehlende Werte (null im Wiki)", "missing"),
                        ("Hinweise", "notes")):
        lines += ["", f"## {title}", ""]
        entries = [x for tid in sorted(logs) for x in getattr(logs[tid], attr)]
        lines += [f"- {x}" for x in entries] or ["- keine"]
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
    n = lambda attr: sum(len(getattr(l, attr)) for l in logs.values())
    print(f"{len(docs)} Türme -> {OUT.relative_to(ROOT)}  |  unparsed: {n('unparsed')}  |  "
          f"Camo-Ausnahmen: {n('camo_exceptions')}  |  Schema-Fehler: {len(errors)}  |  "
          f"Report: {REPORT.relative_to(ROOT)}")
    return 1 if errors and "nicht installiert" not in errors[0] else 0


if __name__ == "__main__":
    sys.exit(main())
