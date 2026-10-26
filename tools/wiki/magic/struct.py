import json,sys
TIERS=['_000','_100','_200','_300','_400','_500','_010','_020','_030','_040','_050','_001','_002','_003','_004','_005']
def names(o,depth=0):
    out=[]
    if isinstance(o,dict):
        for k,v in o.items():
            if k.startswith('_'): continue
            if isinstance(v,dict):
                if '_order' in v:
                    out.append(k+'['+','.join(f"{n}{{{','.join(names(v[n],depth+1))}}}" for n in v['_order'] if n in v)+']')
                else:
                    sub=names(v,depth+1)
                    if sub: out.append(k+'{'+','.join(sub)+'}')
    return out
j=json.load(open(sys.argv[1]))
print(j.get('_last_updated'))
for t in TIERS:
    print(t, '; '.join(names(j[t])))
