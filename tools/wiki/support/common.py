import json,os
import os
OUT = os.path.join(os.path.dirname(__file__), '..', '..', '..', 'data', 'towers') + os.sep
S = os.environ.get('WIKI_RAW_DIR', 'wiki_raw') + os.sep  # Ordner mit den Wiki-Rohdaten
def upgrades(key, changes, extra_desc=None):
    """changes: dict {(path,tier): changes dict}; descriptions from wiki short + detail"""
    u=json.load(open(S+'upg_%s.json'%key))
    names={'1':'top','2':'middle','3':'bottom'}
    res={}
    for p,lst in u.items():
        arr=[]
        for x in lst:
            if x['tier']>5: continue
            d=x['short']
            if x['detail']: d+=' Wiki: '+x['detail']
            d=d.replace(' Paragon','').strip()
            if extra_desc and (int(p),x['tier']) in extra_desc: d+=' NOTE: '+extra_desc[(int(p),x['tier'])]
            arr.append({'tier':x['tier'],'name':x['name'],'cost':x['cost'],'description':d,'changes':changes.get((int(p),x['tier']),{})})
        res[names[p]]=arr
    return res
def atk(name,damage=None,pierce=None,cooldown=None,damage_type=None,projectiles=1,blast_radius=None,notes=""):
    return dict(name=name,damage=damage,pierce=pierce,cooldown=cooldown,damage_type=damage_type,projectiles=projectiles,blast_radius=blast_radius,notes=notes)
def write(fname,obj):
    p=OUT+fname
    json.dump(obj,open(p,'w'),indent=2,ensure_ascii=False)
    print('wrote',p)
