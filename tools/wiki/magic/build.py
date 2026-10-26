"""Build tower JSON from bloonswiki stats-module data + cargo upgrade data.
usage: python3 -I build.py <stats_data.json> <upgrades_cargo.json> <tower name> <base cost> <out.json> [overrides.json]
"""
import json, sys

DT = {0: "normal", 1: "shatter", 2: "explosion", 4: "glacier", 5: "cold", 8: "fire",
      12: "frigid", 17: "sharp", 64: "acid", 72: "plasma", 73: "energy"}
TIERS = {"top": ["_100", "_200", "_300", "_400", "_500"],
         "middle": ["_010", "_020", "_030", "_040", "_050"],
         "bottom": ["_001", "_002", "_003", "_004", "_005"]}
PATHNUM = {"top": "1", "middle": "2", "bottom": "3"}

MOD_LABELS = [
    ("damageModifierForMoabs", "MOAB-class"), ("damageModifierForMoab", "MOABs"),
    ("damageModifierForBad", "BADs"), ("damageModifierForBoss", "bosses"),
    ("damageModifierForCeramic", "ceramic"), ("damageModifierForLead", "lead"),
    ("damageModifierForLeadOrDdt", "lead/DDT"), ("damageModifierForCamo", "camo"),
    ("damageModifierForFortified", "fortified"), ("damageModifierForFortifiedMoabs", "fortified MOAB-class"),
    ("damageModifierForFortifiedOrCeramic", "fortified/ceramic"), ("damageModifierForStunned", "stunned"),
    ("damageModifierForStunnedMoabs", "stunned MOAB-class"), ("damageModifierForFrozen", "frozen"),
    ("damageModifierForStickied", "stickied"), ("damageModifierForRegrow", "regrow"),
    ("damageModifierForShocked1", "shocked(t2)"), ("damageModifierForShocked2", "shocked(t3)"),
    ("damageModifierForShocked3", "shocked(t4)"), ("damageModifierForShocked4", "shocked(t5)"),
    ("damageModifierUnstableConcoction", "unstable concoction"),
]
NOTE_KEYS = ["critDamage", "critLower", "critUpper", "pushAmount", "seekDistance", "lifespan", "speed",
             "maxPierce", "clearHitBloonsInterval", "refreshPierceInterval", "rounds", "randomAngle", "angle",
             "bounceDistance", "maxBounces", "ceramicMultiplier", "multiplierForMoabs", "multiplierForCeramic",
             "multiplierForLead", "damagePercent", "glueLevel", "layers", "interval", "chance",
             "piercePenaltyForMoabs", "piercePenaltyForCeramic", "filterOutMoabs", "filterOutNonMoabs",
             "filterOutLead", "filterOutBfb", "filterOutZomg", "filterOutDdt", "filterOutBad", "filterOutBoss",
             "filterOutNonCamo", "cleanseCamo", "cleanseFortified", "cleanseLead", "cleanseRegen",
             "increaseBloonWorthMultiplier", "increaseBloonWorthLeadCash", "zomgMultiplier",
             "orbitRadius", "spreadLength", "distanceMax", "distanceMin", "canFreezeMoabs",
             "scaleDamagePerSecond", "maxScaleDamage", "scaleProjectilePerSecond", "maxScaleProjectile",
             "maxDotDamage", "affectMoab", "mutationId"]


def num(v):
    if isinstance(v, float) and v.is_integer():
        return int(v)
    return v


def kv_summary(d, skip=()):
    parts = []
    for k, v in d.items():
        if k.startswith("_") or k in skip or isinstance(v, dict):
            continue
        if isinstance(v, bool) or isinstance(v, (int, float)) or isinstance(v, str):
            parts.append(f"{k}={v}")
    return ", ".join(parts)


SKIPK = {"eject", "fireWithoutTarget", "fireBetweenRounds", "attackThroughWalls", "framesBeforeRetarget",
         "sharedGridRange", "ignoreBlockers", "usePointCollisionWithBloons", "canCollisionBeBlockedByMapLos",
         "vsBlockerRadius", "distributeToChildren", "overrideDistributeBlocker", "ignoreImmunityDestroy",
         "maxDamage", "targetFriendlyIgnoreList", "overrideRotation", "ignoreSeekAngle", "useLifetimeAsDistance",
         "constantlyAquireNewTarget", "trackNewTargets"}


def deep_summary(d, depth=0):
    """summarize scalar fields and ordered child groups recursively"""
    parts = []
    for k, v in d.items():
        if k.startswith("_") or k in SKIPK:
            continue
        if isinstance(v, dict):
            if "_order" in v:
                for n in v["_order"]:
                    if n in v and isinstance(v[n], dict):
                        sub = deep_summary(v[n], depth + 1)
                        parts.append(f"{k[:-1] if k.endswith('s') else k} '{n}' {{{sub}}}")
            else:
                parts.append(f"{k} {{{deep_summary(v, depth + 1)}}}")
        elif v not in (None, "", False) and not (isinstance(v, (int, float)) and not isinstance(v, bool) and v == 0):
            if k == "immuneBloonProperties":
                parts.append(f"damage type={DT.get(v, v)}")
            else:
                parts.append(f"{k}={num(v)}")
    return ", ".join(parts)


def effects_notes(proj):
    out = []
    eff = proj.get("effects")
    if isinstance(eff, dict):
        for n in eff.get("_order", []):
            e = eff.get(n, {})
            s = ", ".join(f"{k}={num(v)}" for k, v in e.items() if not k.startswith("_") and not isinstance(v, dict)
                          and k not in ("immuneBloonProperties", "distributeToChildren") and v not in (0, False, None, ""))
            if "immuneBloonProperties" in e:
                s += f" (damage type {DT.get(e['immuneBloonProperties'], e['immuneBloonProperties'])})"
            out.append(f"effect '{n}': {s}")
    return out


def projectile_attack(aname, att, pname, proj, nproj, prefix=""):
    name = aname if (nproj == 1 or pname == "Projectile") else f"{aname}/{pname}"
    name = prefix + name
    notes = []
    dmg = proj.get("damage")
    if dmg is None:
        notes.append("no direct damage listed")
    for k, label in MOD_LABELS:
        if proj.get(k) is True:
            notes.append(f"flag {k}=true")
        elif proj.get(k):
            notes.append(f"+{num(proj[k])} damage vs {label}")
    for k in NOTE_KEYS:
        if k in proj and k not in ("lifespan", "speed", "maxPierce") and proj[k] not in (False, 0, None):
            notes.append(f"{k}={num(proj[k])}")
    if proj.get("maxPierce") not in (None, 0, -1):
        notes.append(f"maxPierce={proj['maxPierce']}")
    if proj.get("lifespan") is not None:
        notes.append(f"lifespan={num(proj['lifespan'])}s")
    if proj.get("speed"):
        notes.append(f"speed={num(proj['speed'])}")
    hit_camo = proj.get("filterInvisible") is not True
    tgt_camo = att.get("filterInvisible") is not True
    if hit_camo and tgt_camo:
        notes.append("can target/hit camo")
    if att.get("filterOutNonCamo") or proj.get("filterOutNonCamo"):
        notes.append("camo only")
    if att.get("targetTypes"):
        notes.append(f"targeting: {att['targetTypes']}")
    if att.get("range") is not None:
        notes.append(f"attack range={num(att['range'])}")
    notes += effects_notes(proj)
    other = {k: v for k, v in proj.items() if isinstance(v, dict) and k != "effects"}
    for k, v in other.items():
        notes.append(f"{k}: " + deep_summary({k: v}))
    for k, v in att.items():
        if isinstance(v, dict) and k != "projectiles":
            notes.append("attack " + deep_summary({k: v}))
    for k in ("initialDelay", "customStartCooldown", "charges"):
        if att.get(k):
            notes.append(f"attack {k}={num(att[k])}")
    dt = proj.get("immuneBloonProperties")
    count = proj.get("count", att.get("count", 1))
    rate = att.get("rate")
    if rate is not None and rate >= 9999:
        notes.append(f"attack rate field={rate} (effectively not periodic)")
    radius = proj.get("radius")
    blast = None
    if any(w in pname.lower() for w in ("explosion", "blast", "bomb")) and pname != "Projectile":
        blast = num(radius)
    else:
        if radius is not None:
            notes.append(f"projectile radius={num(radius)}")
    return {
        "name": name,
        "damage": num(dmg) if dmg is not None else 0,
        "pierce": num(proj.get("pierce")) if proj.get("pierce") is not None else None,
        "cooldown": num(rate) if rate is not None else None,
        "damage_type": DT.get(dt, None) if dt is not None else None,
        "projectiles": num(count),
        "blast_radius": blast,
        "notes": "; ".join(notes),
    }


def attacks_of(node, prefix=""):
    res = []
    atts = node.get("attacks")
    if isinstance(atts, dict):
        for an in atts.get("_order", []):
            att = atts.get(an)
            if not isinstance(att, dict):
                continue
            projs = att.get("projectiles", {})
            order = [p for p in projs.get("_order", []) if p in projs] if isinstance(projs, dict) else []
            if not order:
                res.append({"name": prefix + an, "damage": 0, "pierce": None, "cooldown": num(att.get("rate")),
                            "damage_type": None, "projectiles": num(att.get("count", 1)), "blast_radius": None,
                            "notes": "no projectile data; " + deep_summary(att)})
            for pn in order:
                res.append(projectile_attack(an, att, pn, projs[pn], len(order), prefix))
    return res


def all_attacks(node):
    res = attacks_of(node)
    subs = node.get("subtowers")
    if isinstance(subs, dict):
        for sn in subs.get("_order", []):
            st = subs.get(sn, {})
            sa = attacks_of(st, prefix=f"{sn}: ")
            for a in sa:
                a["notes"] = (f"attack of sub-tower '{sn}'"
                              + (f" (sub-tower lifespan {num(st['lifespan'])}s)" if st.get("lifespan") else "")
                              + "; " + a["notes"])
            res += sa
    return res


def abilities_of(node):
    res = []
    ab = node.get("abilities")
    if isinstance(ab, dict):
        for n in ab.get("_order", []):
            a = ab.get(n, {})
            eff = []
            projs = a.get("projectiles")
            if isinstance(projs, dict):
                for pn in projs.get("_order", []):
                    if pn in projs:
                        pa = projectile_attack(n, {}, pn, projs[pn], len(projs["_order"]))
                        eff.append(pa)
            for x in attacks_of(a):
                eff.append(x)
            extra = kv_summary(a, skip=("cooldown", "maxActivationsPerRound", "canActivateBetweenRounds",
                                        "restrictAbilityAfterMaxRoundTimer"))
            summ = "; ".join(f"{e['name']}: damage {e['damage']}, pierce {e['pierce']}, type {e['damage_type']}, "
                             f"count {e['projectiles']} ({e['notes']})" for e in eff)
            if extra:
                summ = (summ + "; " if summ else "") + extra
            for grp in ("buffs", "zones", "effects", "subtowers"):
                g = a.get(grp)
                if isinstance(g, dict):
                    for gn in g.get("_order", []):
                        if gn in g:
                            summ += f"; {grp[:-1]} '{gn}': " + kv_summary(g[gn])
            res.append({"name": n, "cooldown": num(a.get("cooldown")), "summary": summ})
    return res


def misc_of(node):
    """buffs / zones / other named groups"""
    out = {}
    known = {"placeableOnLand", "placeableOnWater", "placeableOnTrack", "range", "towerSelectionMenuThemeId",
             "footprintRadius", "doesntBlockTowerPlacement", "targetTypeFirst", "targetTypeLast",
             "targetTypeClose", "targetTypeStrong", "footprintX", "footprintY"}
    for k, v in node.items():
        if not k.startswith("_") and k not in known and not isinstance(v, dict):
            out[k] = num(v)
    for grp in ("buffs", "zones", "collectables", "effects"):
        g = node.get(grp)
        if isinstance(g, dict):
            for n in g.get("_order", []):
                if n in g:
                    out[f"{grp[:-1]}:{n}"] = deep_summary(g[n])
    return out


def camo_of(node):
    atts = node.get("attacks", {})
    order = atts.get("_order", []) if isinstance(atts, dict) else []
    if not order:
        return None
    a = atts[order[0]]
    projs = a.get("projectiles", {})
    po = projs.get("_order", []) if isinstance(projs, dict) else []
    proj_ok = all(projs[p].get("filterInvisible") is not True for p in po if p in projs) if po else True
    return a.get("filterInvisible") is not True and proj_ok


def fmt_delta(old, new, mult=False):
    if old is None or new is None:
        return new
    if mult:
        if old == 0:
            return new
        r = new / old
        return f"x{round(r, 4):g}"
    d = new - old
    d = round(d, 4)
    return f"{'+' if d >= 0 else ''}{num(d):g}" if isinstance(d, float) else f"{'+' if d >= 0 else ''}{d}"


def diff(prev, cur, main_name):
    ch = {}
    if prev["range"] != cur["range"]:
        ch["range"] = fmt_delta(prev["range"], cur["range"])
    if prev["camo"] != cur["camo"]:
        ch["camo"] = cur["camo"]
    pa = {a["name"]: a for a in prev["attacks"]}
    ca = {a["name"]: a for a in cur["attacks"]}
    new = []
    for n, a in ca.items():
        if n not in pa:
            new.append(a)
            continue
        o = pa[n]
        pre = "" if n == main_name else f"{n}."
        for k in ("damage", "pierce", "projectiles", "blast_radius"):
            if o[k] != a[k]:
                ch[pre + k] = fmt_delta(o[k], a[k]) if isinstance(o[k], (int, float)) and isinstance(a[k], (int, float)) else a[k]
        if o["cooldown"] != a["cooldown"]:
            ch[pre + "cooldown"] = fmt_delta(o["cooldown"], a["cooldown"], mult=True) if isinstance(o["cooldown"], (int, float)) and isinstance(a["cooldown"], (int, float)) else a["cooldown"]
        if o["damage_type"] != a["damage_type"]:
            ch[pre + "damage_type"] = a["damage_type"]
        if o["notes"] != a["notes"]:
            ch[pre + "notes"] = a["notes"]
    if len(new) == 1:
        ch["new_attack"] = new[0]
    elif new:
        ch["new_attacks"] = new
    removed = [n for n in pa if n not in ca]
    if removed:
        ch["removed_attacks"] = removed
    pab = {a["name"]: a for a in prev["abilities"]}
    for a in cur["abilities"]:
        if a["name"] not in pab:
            ch["new_ability"] = {"name": a["name"], "cooldown": a["cooldown"], "effect_stats": a["summary"]}
        else:
            if pab[a["name"]]["cooldown"] != a["cooldown"]:
                ch[f"ability.{a['name']}.cooldown"] = a["cooldown"]
            if pab[a["name"]]["summary"] != a["summary"]:
                ch[f"ability.{a['name']}.effect_stats"] = a["summary"]
    for k, v in cur["misc"].items():
        if prev["misc"].get(k) != v:
            ch[k] = v
    return ch


def snapshot(node):
    return {"range": num(node.get("range")), "camo": camo_of(node), "attacks": all_attacks(node),
            "abilities": abilities_of(node), "misc": misc_of(node)}


def main():
    data = json.load(open(sys.argv[1]))
    cargo = json.load(open(sys.argv[2]))
    tower, cost, out = sys.argv[3], int(sys.argv[4]), sys.argv[5]
    ov = json.load(open(sys.argv[6])) if len(sys.argv) > 6 else {}
    ups = {}
    for r in cargo["cargoquery"]:
        r = r["title"]
        if r["tower"] == tower and r["unused"] == "0" and r["tier"] in "12345":
            ups[(r["path"], r["tier"])] = r
    base = data["_000"]
    snap0 = snapshot(base)
    main_name = snap0["attacks"][0]["name"] if snap0["attacks"] else None
    land, water = base.get("placeableOnLand"), base.get("placeableOnWater")
    placement = "land_or_water" if land and water else ("water" if water else "land")
    res = {
        "name": tower, "category": "magic", "cost": cost,
        "footprint_radius": num(base.get("footprintRadius")), "placement": placement,
        "range": snap0["range"], "camo": snap0["camo"], "attacks": snap0["attacks"],
        "abilities": [], "upgrades": {}, "source_urls": [f"https://www.bloonswiki.com/{tower.replace(' ', '_')}_(BTD6)",
                        f"https://www.bloonswiki.com/Stats:{tower.replace(' ', '_')}_(BTD6)",
                        f"https://www.bloonswiki.com/Module:BTD6_stats/{tower.replace(' ', '_')}/new"], "game_version": data.get("_last_updated"),
    }
    if snap0["misc"]:
        print("BASE MISC:", tower, snap0["misc"])
        if res["attacks"]:
            res["attacks"][0]["notes"] += "; tower-level passive (wiki data): " + json.dumps(snap0["misc"])
    abil_desc = {}
    for path, keys in TIERS.items():
        prev = snap0
        lst = []
        for i, k in enumerate(keys, 1):
            cur = snapshot(data[k])
            u = ups.get((PATHNUM[path], str(i)))
            ch = diff(prev, cur, main_name)
            entry = {"tier": i, "name": u["name"] if u else None, "cost": int(u["cost"]) if u else None,
                     "description": u["description"] if u else "upgrade not found in wiki cargo data",
                     "changes": ch}
            lst.append(entry)
            for a in cur["abilities"]:
                if a["name"] not in [b["name"] for b in prev["abilities"]] or not any(
                        x["name"] == f"{a['name']}" and x["from"] == f"{path}{i}" for x in []):
                    pass
            for a in cur["abilities"]:
                if a["name"] not in [b["name"] for b in prev["abilities"]]:
                    res["abilities"].append({"name": f"{u['name'] if u else k} ({a['name']})",
                                             "cooldown": a["cooldown"],
                                             "description": (u["description"] if u else "") +
                                             f" [unlocked by {path} path tier {i}]" +
                                             (f" Effect stats (wiki data at unlock tier): {a['summary']}" if a["summary"] else "")})
            prev = cur
        res["upgrades"][path] = lst
    # overrides
    for k, v in ov.get("top_level", {}).items():
        res[k] = v
    for path, tiers in ov.get("upgrades", {}).items():
        for t, patch in tiers.items():
            e = res["upgrades"][path][int(t) - 1]
            for pk, pv in patch.items():
                if pk == "changes_merge":
                    e["changes"].update(pv)
                elif pk == "description_append":
                    e["description"] += " " + pv
                else:
                    e[pk] = pv
    res["abilities"] += ov.get("abilities_append", [])
    for i, patch in ov.get("abilities_patch", {}).items():
        res["abilities"][int(i)].update(patch)
    json.dump(res, open(out, "w"), indent=2, ensure_ascii=False)


if __name__ == "__main__":
    main()
