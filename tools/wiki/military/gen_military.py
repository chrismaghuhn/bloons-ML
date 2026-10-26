"""Generate BTD6 Military tower JSON files from downloaded bloonswiki data.
Usage: python3 -I gen_military.py <raw_dir> <out_dir>
"""
import json, re, sys, os

RAW, OUT = sys.argv[1], sys.argv[2]
TOWERS = ["Sniper Monkey", "Monkey Sub", "Monkey Buccaneer", "Monkey Ace",
          "Heli Pilot", "Mortar Monkey", "Dartling Gunner"]
DT = {0: "normal", 1: "shatter", 2: "explosion", 4: "glacier", 5: "cold", 8: "fire",
      12: "frigid", 17: "sharp", 64: "acid", 72: "plasma", 73: "energy"}
BIG = 999999
PATHS = {"top": ["_100", "_200", "_300", "_400", "_500"],
         "middle": ["_010", "_020", "_030", "_040", "_050"],
         "bottom": ["_001", "_002", "_003", "_004", "_005"]}
PATHNUM = {"top": "1", "middle": "2", "bottom": "3"}


def load_module(t):
    d = json.load(open(os.path.join(RAW, "mod_%s.json" % t.replace(" ", "_"))))
    return json.loads(list(d["query"]["pages"].values())[0]["revisions"][0]["slots"]["main"]["*"])


def clean_wikitext(s):
    s = re.sub(r"\[\[(?:[^\]|]*\|)?([^\]]*)\]\]", r"\1", s)
    s = re.sub(r"\{\{[^{}|]*\|([^{}]*)\}\}", lambda m: m.group(1).split("|")[0], s)
    s = re.sub(r"\{\{([^{}]*)\}\}", r"\1", s)
    s = re.sub(r"'''?", "", s)
    s = re.sub(r"<[^>]+>", "", s)
    return s.strip()


def num(v):
    if isinstance(v, float) and v.is_integer():
        return int(v)
    return v


def items(container):
    if not isinstance(container, dict):
        return []
    order = container.get("_order") or [k for k in container if not k.startswith("_")]
    return [(k, container[k]) for k in order if isinstance(container.get(k), dict)]


def effect_note(name, e):
    keep = []
    for k, v in e.items():
        if k.startswith("_") or isinstance(v, dict):
            continue
        if k in ("distributeToChildren", "triggerImmediate", "chance", "glueLevel") and not v:
            continue
        if k == "immuneBloonProperties":
            v = DT.get(v, v)
            k = "damage_type"
        if v in ("", None):
            continue
        keep.append("%s=%s" % (k, num(v)))
    return "%s(%s)" % (name, ", ".join(keep))


def proj_notes(p):
    n = []
    for k, v in p.items():
        if k.startswith("damageModifierFor") and v:
            n.append("+%s damage vs %s" % (num(v), k[len("damageModifierFor"):]))
    if p.get("filterOutNonMoabs"):
        n.append("hits MOAB-class only")
    if p.get("filterOutMoabs"):
        n.append("cannot hit MOAB-class")
    if p.get("filterOutLead"):
        n.append("cannot hit Lead")
    if p.get("filterOutDdt"):
        n.append("cannot hit DDT")
    if p.get("filterInvisible"):
        n.append("cannot hit Camo")
    if p.get("filterAllExceptTarget"):
        n.append("hits only target (instant)")
    if p.get("maxDamage") and 0 < p["maxDamage"] < BIG:
        n.append("max damage %s" % num(p["maxDamage"]))
    if p.get("maxPierce") and 0 < p["maxPierce"] < BIG:
        n.append("max pierce %s" % num(p["maxPierce"]))
    if "seekDistance" in p or "maxTurnRate" in p or "turnRate" in p:
        n.append("seeking")
    if p.get("maxBounces"):
        n.append("bounces %s times (range %s)" % (num(p["maxBounces"]), num(p.get("bounceDistance"))))
    for c in ("cleanseCamo", "cleanseRegen", "cleanseLead", "cleanseFortified"):
        if p.get(c):
            n.append("removes " + c[len("cleanse"):] + (" (excl. %s)" % p["cleanseExcludeList"] if p.get("cleanseExcludeList") else ""))
    if p.get("speed"):
        n.append("speed %s" % num(p["speed"]))
        if p.get("radius"):
            n.append("collision radius %s" % num(p["radius"]))
    if p.get("lifespan") and p["lifespan"] >= 0.5:
        n.append("lifespan %ss" % num(p["lifespan"]))
    if p.get("angle"):
        n.append("spread angle %s" % num(p["angle"]))
    for en, e in items(p.get("effects")):
        n.append(effect_note(en, e))
    return n


def attack_objs(attacks, prefix=""):
    out = []
    for an, a in items(attacks):
        rate = a.get("rate")
        acount = a.get("count", 1) or 1
        projs = items(a.get("projectiles"))
        n_before = len(out)
        for pn, p in projs:
            dmg = p.get("damage")
            has_effect = bool(items(p.get("effects")))
            cleanse = any(p.get(c) for c in ("cleanseCamo", "cleanseRegen", "cleanseLead", "cleanseFortified"))
            if not dmg and not has_effect and not cleanse:
                continue  # carrier projectile with no damage (e.g. missile body)
            pierce = p.get("pierce")
            notes = proj_notes(p)
            if a.get("filterOutNonMoabs"):
                notes.append("attack targets MOAB-class only")
            if a.get("rateMin"):
                notes.append("min cooldown %s" % num(a["rateMin"]))
            if a.get("range") is not None and a.get("range") < BIG:
                notes.append("attack range %s" % num(a["range"]))
            if a.get("spread"):
                notes.append("target spread %s" % num(a["spread"]))
            if a.get("rotateRate") and a["rotateRate"] != "\u221e deg/s":
                notes.append("turret rotation %s" % a["rotateRate"])
            r = p.get("radius")
            out.append({
                "name": (prefix + an + "/" + pn),
                "damage": num(dmg) if dmg is not None else None,
                "pierce": num(pierce) if pierce is not None and pierce < BIG else ("infinite" if pierce else None),
                "cooldown": num(rate) if rate is not None and rate < 9999 else None,
                "damage_type": DT.get(p.get("immuneBloonProperties"), None) if "immuneBloonProperties" in p else None,
                "projectiles": num(acount * (p.get("count", 1) or 1)),
                "blast_radius": num(r) if (r and ("xplo" in pn.lower() or "blast" in pn.lower() or not p.get("speed"))) else None,
                "notes": "; ".join(notes),
            })
        if len(out) == n_before:
            notes = []
            if a.get("targetTypes"):
                notes.append("targets: %s" % a["targetTypes"])
            for k, v in a.items():
                if k.startswith("filterOut") and v:
                    notes.append("cannot target " + k[len("filterOut"):])
            notes.append("no damaging projectile listed in wiki data")
            out.append({"name": prefix + an, "damage": None, "pierce": None,
                        "cooldown": num(rate) if rate is not None and rate < 9999 else None,
                        "damage_type": None, "projectiles": num(acount), "blast_radius": None,
                        "notes": "; ".join(notes)})
    return out


def ability_objs(node):
    out = []
    for an, ab in items(node.get("abilities")):
        parts = []
        for k in ("lifespan", "maxActivationsPerRound"):
            if ab.get(k) is not None and ab.get(k) != 0:
                parts.append("%s=%s" % (k, num(ab[k])))
        for att in attack_objs(ab.get("attacks")):
            parts.append("attack %s: dmg %s, pierce %s, cd %s, type %s, x%s%s" % (
                att["name"], att["damage"], att["pierce"], att["cooldown"], att["damage_type"],
                att["projectiles"], (", " + att["notes"]) if att["notes"] else ""))
        for pn, p in items(ab.get("projectiles")):
            parts.append("projectile %s: dmg %s, pierce %s, radius %s, type %s%s" % (
                pn, num(p.get("damage")), num(p.get("pierce")), ("global" if (p.get("radius") or 0) >= BIG else num(p.get("radius"))),
                DT.get(p.get("immuneBloonProperties")), ("; " + "; ".join(proj_notes(p))) if proj_notes(p) else ""))
        for cn, c in items(ab.get("collectables")):
            parts.append("collectable %s: %s" % (cn, ", ".join("%s=%s" % (k, num(v)) for k, v in c.items() if not k.startswith("_") and not isinstance(v, dict) and v not in (0, False, "", None) and k not in ("rounds", "delay", "emittedByCashEarnedMultiplier"))))
        for bn, b in items(ab.get("buffs")):
            parts.append("buff %s: %s" % (bn, ", ".join("%s=%s" % (k, num(v)) for k, v in b.items() if not k.startswith("_") and not isinstance(v, dict))))
        for sn, s in items(ab.get("subtowers")):
            parts.append("spawns %s" % sn)
            for att in attack_objs(s.get("attacks")):
                parts.append("  %s attack %s: dmg %s, pierce %s, cd %s, type %s, x%s" % (
                    sn, att["name"], att["damage"], att["pierce"], att["cooldown"], att["damage_type"], att["projectiles"]))
        out.append({"name": an, "cooldown": num(ab.get("cooldown")), "description": "; ".join(parts)})
    return out


def misc(node):
    """Non-attack behaviours: buffs, zones, collectables, cash, subtowers."""
    m = {}
    for bn, b in items(node.get("buffs")):
        m["buff:" + bn] = {k: num(v) for k, v in b.items() if not k.startswith("_") and not isinstance(v, dict)}
    for zn, z in items(node.get("zones")):
        m["zone:" + zn] = {k: num(v) for k, v in z.items() if not k.startswith("_") and not isinstance(v, dict)}
    for cn, c in items(node.get("collectables")):
        m["collectable:" + cn] = {k: num(v) for k, v in c.items() if not k.startswith("_") and not isinstance(v, dict)}
    if node.get("cashPerRound"):
        m["cash_per_round"] = num(node["cashPerRound"])
    if node.get("speed") is not None:
        m["movement_speed"] = num(node["speed"])
    return m


def subtower_attacks(node):
    out = []
    for sn, s in items(node.get("subtowers")):
        for att in attack_objs(s.get("attacks"), prefix="[%s] " % sn):
            att["notes"] = ("subtower %s (range %s); " % (sn, num(s.get("range"))) + att["notes"]).rstrip("; ")
            out.append(att)
    return out


def snapshot(node):
    camo = any(not a.get("filterInvisible", True) for _, a in items(node.get("attacks"))) \
        or any(not a.get("filterInvisible", True) for _, s in items(node.get("subtowers")) for _, a in items(s.get("attacks")))
    return {
        "range": num(node.get("range")),
        "camo": camo,
        "attacks": attack_objs(node.get("attacks")) + subtower_attacks(node),
        "abilities": ability_objs(node),
        "misc": misc(node),
        "targeting": sorted(k[len("targetType"):] for k, v in node.items() if k.startswith("targetType") and v),
    }


def rel(old, new, key):
    if isinstance(old, (int, float)) and isinstance(new, (int, float)) and not isinstance(old, bool):
        if key == "cooldown":
            return "x%s (%s -> %s)" % (round(new / old, 3) if old else "?", old, new)
        d = new - old
        d = round(d, 4)
        return ("+%s" % num(d)) if d >= 0 else "%s" % num(d)
    return new


def diff(prev, cur):
    ch = {}
    if prev["range"] != cur["range"]:
        ch["range"] = rel(prev["range"], cur["range"], "range")
    if prev["camo"] != cur["camo"]:
        ch["camo"] = cur["camo"]
    pa = {a["name"]: a for a in prev["attacks"]}
    ca = {a["name"]: a for a in cur["attacks"]}
    new_attacks = [a for n, a in ca.items() if n not in pa]
    removed = [n for n in pa if n not in ca]
    for n, a in ca.items():
        if n in pa:
            for k in ("damage", "pierce", "cooldown", "damage_type", "projectiles", "blast_radius", "notes"):
                if pa[n][k] != a[k]:
                    if k == "notes":
                        ch["%s.notes" % n] = a[k]
                    else:
                        ch["%s.%s" % (n, k)] = rel(pa[n][k], a[k], k)
    if len(new_attacks) == 1:
        ch["new_attack"] = new_attacks[0]
    elif new_attacks:
        ch["new_attacks"] = new_attacks
    if removed:
        ch["removed_attacks"] = removed
    pab = {a["name"]: a for a in prev["abilities"]}
    for a in cur["abilities"]:
        if a["name"] not in pab:
            ch.setdefault("new_ability", []).append(a)
        elif pab[a["name"]] != a:
            ch.setdefault("changed_ability", []).append(a)
    for k, v in cur["misc"].items():
        if prev["misc"].get(k) != v:
            ch[k] = v if k not in prev["misc"] or not isinstance(v, (int, float)) else rel(prev["misc"][k], v, k)
    for k in prev["misc"]:
        if k not in cur["misc"]:
            ch[k] = "removed"
    nt = [t for t in cur["targeting"] if t not in prev["targeting"]]
    if nt:
        ch["new_targeting_options"] = nt
    return ch


def html_upgrades(t):
    import html as H
    d = json.load(open(os.path.join(RAW, "html_%s.json" % t.replace(" ", "_"))))["parse"]["text"]["*"]
    res = {}
    for blk in d.split('<div class="ur-wrapper">')[1:]:
        name = re.search(r'<div class="ur-head"><a [^>]*>([^<]*)</a>', blk)
        med = re.search(r'\$([\d,]+) \u2014 <a href="/Medium"', blk)
        long_ = re.search(r'<div class="flex-item-long flex-item">(.*?)</div></div></div>', blk, re.S)
        if not name:
            continue
        txt = ""
        if long_:
            txt = re.sub(r"<style.*?</style>", "", long_.group(1), flags=re.S)
            txt = H.unescape(re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", txt))).strip()
        res[H.unescape(name.group(1))] = (int(med.group(1).replace(",", "")) if med else None, txt)
    return res


CAMO_OVERRIDES = {
    # The wiki upgrade text says Increased Accuracy grants camo detection, while the
    # stats module still shows filterInvisible=true until Signal Flare.
    ("Mortar Monkey", "bottom", 1): "Wiki upgrade text states the Mortar gains camo detection at this tier; the wiki stats module (filterInvisible) only shows camo from Signal Flare (0-0-3). Camo set true here following the upgrade text.",
}


def main():
    up = json.load(open(os.path.join(RAW, "upgrades.json")))["cargoquery"]
    up = [r["title"] for r in up]
    towers = {r["title"]["name"]: r["title"] for r in json.load(open(os.path.join(RAW, "towers.json")))["cargoquery"]}
    for t in TOWERS:
        m = load_module(t)
        hdata = html_upgrades(t)
        base = m["_000"]
        bs = snapshot(base)
        land, water = base.get("placeableOnLand"), base.get("placeableOnWater")
        placement = "land_or_water" if land and water else ("water" if water else "land")
        fp = base.get("footprintRadius")
        out = {
            "name": t,
            "category": "military",
            "cost": int(towers[t]["cost"]),
            "footprint_radius": num(fp) if fp is not None else None,
            "placement": placement,
            "range": bs["range"],
            "camo": bs["camo"],
            "attacks": bs["attacks"],
            "abilities": [],
            "upgrades": {},
            "source_urls": [
                "https://www.bloonswiki.com/%s_(BTD6)" % t.replace(" ", "_"),
                "https://www.bloonswiki.com/Module:BTD6_stats/%s/new" % t.replace(" ", "_"),
                "https://www.bloonswiki.com/api.php?action=cargoquery&tables=btd6_upgrades (tower=%s)" % t,
            ],
            "game_version": str(m.get("_last_updated")),
        }
        if fp is None and base.get("footprintX") is not None:
            out["footprint_radius"] = None
        for path, keys in PATHS.items():
            lst = []
            prev = bs
            forced = False
            for i, k in enumerate(keys, 1):
                cur = snapshot(m[k])
                row = [r for r in up if r["tower"] == t and r["path"] == PATHNUM[path] and int(r["tier"]) == i and r["unused"] == "0"]
                assert len(row) == 1, (t, path, i, row)
                row = row[0]
                ov = CAMO_OVERRIDES.get((t, path, i))
                if ov:
                    forced = True
                if forced:
                    cur["camo"] = True
                ch = diff(prev, cur)
                hu = hdata.get(row["name"])
                desc = clean_wikitext(row["description"])
                if hu is None:
                    desc += " [Wiki upgrade summary not found on tower page.]"
                else:
                    if hu[0] is not None and hu[0] != int(row["cost"]):
                        desc += " [Cost mismatch: cargo %s vs page Medium %s.]" % (row["cost"], hu[0])
                    if hu[1]:
                        desc += " Wiki: " + hu[1]
                if ov:
                    desc += " [Note: " + ov + "]"
                for ab in ch.get("new_ability", []):
                    out["abilities"].append({"name": "%s (%s)" % (ab["name"], row["name"]), "cooldown": ab["cooldown"],
                                             "description": "Unlocked by %s (%s tier %d): %s Data: %s" % (row["name"], path, i, clean_wikitext(row["description"]), ab["description"])})
                lst.append({"tier": i, "name": row["name"], "cost": int(row["cost"]),
                            "description": desc, "changes": ch})
                prev = cur
            out["upgrades"][path] = lst
        fn = os.path.join(OUT, t.lower().replace(" ", "_") + ".json")
        with open(fn, "w") as f:
            json.dump(out, f, indent=2, ensure_ascii=False)
            f.write("\n")
        print("wrote", fn)


main()
