"""Build tower JSON files from downloaded bloonswiki data.
Usage: python3 -I build.py <raw_dir> <out_dir> <scripts_dir>
"""
import json, sys, re, os
sys.path.insert(0, sys.argv[3])  # only our own scripts dir
from parse_stats import parse

RAW, OUT = sys.argv[1], sys.argv[2]

TOWERS = {
    'Dart_Monkey': ('Dart Monkey', 'dart_monkey', 'dart'),
    'Boomerang_Monkey': ('Boomerang Monkey', 'boomerang_monkey', 'boomerang'),
    'Bomb_Shooter': ('Bomb Shooter', 'bomb_shooter', 'bomb'),
    'Tack_Shooter': ('Tack Shooter', 'tack_shooter', 'tacks'),
    'Ice_Monkey': ('Ice Monkey', 'ice_monkey', 'freeze'),
    'Glue_Gunner': ('Glue Gunner', 'glue_gunner', 'glue'),
    'Desperado': ('Desperado', 'desperado', 'revolver'),
}
ABILITY_HEADS = {'Ability', 'Take Aim', 'Marked to Pop'}
PASSIVE_HEADS = {'Arctic Wind', 'Nomad buff'}
PATHS = ['top', 'middle', 'bottom']

def num(s):
    if s is None: return None
    s = s.strip()
    m = re.match(r'^[×x]?([+-]?[\d,]*\.?\d+)\s*(s|units|unit\(s\)|units/s|%|°)?$', s)
    if not m: return None
    v = float(m.group(1).replace(',', ''))
    return int(v) if v == int(v) and '.' not in m.group(1) else v

def prop(block, key):
    for k, v in block['props']:
        if k == key: return v
    return None

def dtype(s):
    if not s: return None
    return re.sub(r'\s*\[.*\]', '', s).strip().lower()

SKIP_KEYS = {'Offset (X, Y, Z)', 'Offset override (X, Y, Z)'}
def ser(block, skip=()):
    parts = []
    for k, v in block['props']:
        if k in SKIP_KEYS or k in skip: continue
        if k == 'Targeting' and v == 'Depends on targeting option': continue
        parts.append(f'{k}: {v}')
    for c in block['children']:
        parts.append(f"{c['head']} {{{ser(c)}}}")
    return '; '.join(parts)

def make_attack(block, name, tower_range):
    kids = block['children']
    first = kids[0] if kids else None
    expl = next((c for c in kids if c['head'] == 'Explosion'), None)
    if expl is not None and prop(expl, 'Damage') is not None and first is not None and prop(first, 'Damage') is None:
        primary = expl  # bomb-style: carrier projectile does no damage, explosion does
    else:
        primary = next((c for c in kids if prop(c, 'Damage') is not None), None)
    src = primary or first
    dmg = num(prop(primary, 'Damage')) if primary else 0
    pierce = num(prop(src, 'Pierce')) if src else None
    count = num(prop(first, 'Count')) if first and prop(first, 'Count') else 1
    filters = prop(block, 'Filters') or ''
    trange = prop(block, 'Targeting range')
    note_parts = []
    if not primary:
        note_parts.append('no direct damage listed on wiki')
    if trange and num(trange) != tower_range:
        note_parts.append(f'targeting range {trange}')
    note_parts.append('can target Camo' if 'Cannot target Camo' not in filters else 'cannot target Camo')
    note_parts.append('wiki stats: ' + ser(block))
    return {
        'name': name,
        'damage': dmg,
        'pierce': pierce,
        'cooldown': num(prop(block, 'Cooldown')),
        'damage_type': dtype(prop(primary, 'Damage type')) if primary else None,
        'projectiles': count,
        'blast_radius': num(prop(expl, 'Radius')) if expl else None,
        'notes': '; '.join(note_parts),
    }

def snapshot(blocks, main_name, upg_name):
    tower = next(b for b in blocks if b['head'] == 'Tower')
    rng = num(prop(tower, 'Display range'))
    snap = {'range': rng, 'footprint': num(prop(tower, 'Footprint radius')),
            'targeting': prop(tower, 'Targeting options'),
            'attacks': {}, 'abilities': {}, 'passives': {}}
    for b in blocks:
        h = b['head']
        if h == 'Tower': continue
        if h in ABILITY_HEADS:
            nm = upg_name if h == 'Ability' else h
            key = 'Ability' if h == 'Ability' else h
            snap['abilities'][key] = {'name': nm, 'cooldown': num(prop(b, 'Cooldown')),
                                      'stats': ser(b, skip=('Cooldown',))}
        elif h in PASSIVE_HEADS:
            snap['passives'][h] = {'name': h, 'notes': 'wiki stats: ' + ser(b)}
        else:
            nm = main_name if h == 'Attack' else h.lower().replace(' attack', '').replace(' ', '_')
            snap['attacks'][nm] = make_attack(b, nm, rng)
    main = snap['attacks'].get(main_name)
    snap['camo'] = bool(main) and 'can target Camo' in main['notes'].split('; wiki stats')[0]
    return snap

FIELDS = ['damage', 'pierce', 'cooldown', 'damage_type', 'projectiles', 'blast_radius', 'notes']
def diff(prev, cur, main_name, upg_desc):
    ch = {}
    if cur['range'] != prev['range']: ch['range'] = cur['range']
    if cur['footprint'] != prev['footprint']: ch['footprint_radius'] = cur['footprint']
    if cur['targeting'] != prev['targeting']: ch['targeting_options'] = cur['targeting']
    if cur['camo'] != prev['camo']: ch['camo'] = cur['camo']
    for nm, a in cur['attacks'].items():
        if nm in prev['attacks']:
            pa = prev['attacks'][nm]
            d = {f: a[f] for f in FIELDS if a[f] != pa[f]}
            if nm == main_name:
                ch.update(d)
            elif d:
                ch.setdefault('modified_attacks', {})[nm] = d
        else:
            ch.setdefault('new_attacks', []).append(a)
    for nm in prev['attacks']:
        if nm not in cur['attacks']:
            ch.setdefault('removed_attacks', []).append(nm)
    for k, ab in cur['abilities'].items():
        pab = prev['abilities'].get(k)
        if pab is None or pab['cooldown'] != ab['cooldown'] or pab['stats'] != ab['stats']:
            ch.setdefault('abilities', []).append(
                {'name': ab['name'], 'cooldown': ab['cooldown'], 'description': 'wiki stats: ' + ab['stats']})
    for k, ps in cur['passives'].items():
        if prev['passives'].get(k) != ps:
            ch.setdefault('passive_effects', []).append(ps)
    if 'new_attacks' in ch and len(ch['new_attacks']) == 1:
        ch['new_attack'] = ch.pop('new_attacks')[0]
    return ch

def clean_desc(s):
    s = re.sub(r"\[\[(?:[^|\]]*\|)?([^\]]*)\]\]", r'\1', s or '')
    s = re.sub(r"'''?", '', s)
    return s.strip()

def main():
    upg = json.load(open(os.path.join(RAW, 'upgrades.json')))['cargoquery']
    tcost = {r['title']['name']: int(r['title']['cost']) for r in json.load(open(os.path.join(RAW, 'towers.json')))['cargoquery']}
    latest = json.load(open(os.path.join(RAW, 'updates.json')))['cargoquery'][0]['title']['version']
    for key, (name, fname, main_name) in TOWERS.items():
        stats = parse(os.path.join(RAW, f'stats_{key}.html.json'))
        h = json.load(open(os.path.join(RAW, f'stats_{key}.html.json')))['parse']['text']['*']
        m = re.search(r'last updated for:?\s*(?:<[^>]+>\s*)*version\s*([0-9.]+)', h)
        ver = m.group(1) if m else None
        ups = {}
        for r in upg:
            r = r['title']
            if r['tower'] != name or r['unused'] == '1': continue
            p, t = int(r['path']), int(r['tier'])
            if p < 1: continue
            ups[(p, t)] = r
        base = snapshot(stats['000'], main_name, None)
        tower = next(b for b in stats['000'] if b['head'] == 'Tower')
        out = {
            'name': name, 'category': 'primary',
            'cost': tcost.get(name), 'footprint_radius': base['footprint'], 'placement': 'land',
            'range': base['range'], 'camo': base['camo'],
            'attacks': list(base['attacks'].values()),
            'abilities': [],
            'upgrades': {},
            'source_urls': [
                f'https://www.bloonswiki.com/{key}_(BTD6)',
                f'https://www.bloonswiki.com/Stats:{key}_(BTD6)',
                'https://www.bloonswiki.com/Special:CargoTables/btd6_upgrades',
            ],
            'game_version': None,
        }
        out['game_version'] = f'stats last updated for {ver} per wiki Stats page; latest BTD6 version on wiki: {latest}'
        for pi, pname in enumerate(PATHS, start=1):
            prev = base; lst = []
            for t in range(1, 6):
                code = ['0', '0', '0']; code[pi - 1] = str(t); code = ''.join(code)
                u = ups.get((pi, t))
                uname = u['name'] if u else None
                cur = snapshot(stats[code], main_name, uname)
                ch = diff(prev, cur, main_name, None)
                entry = {'tier': t, 'name': uname,
                         'cost': int(u['cost']) if u and u['cost'] else None,
                         'description': clean_desc(u['description']) if u else None,
                         'changes': ch}
                lst.append(entry)
                for ab in ch.get('abilities', []):
                    out['abilities'].append({'name': ab['name'], 'cooldown': ab['cooldown'],
                        'description': f"Unlocked by {uname} ({pname} path tier {t}). In-game text: {clean_desc(u['description']) if u else ''} " + ab['description']})
                prev = cur
            out['upgrades'][pname] = lst
        json.dump(out, open(os.path.join(OUT, fname + '.json'), 'w'), indent=2, ensure_ascii=False)
        print('wrote', fname)

main()
