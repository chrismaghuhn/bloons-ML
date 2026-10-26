import json,sys,collections
c=collections.Counter()
def walk(o,path):
    if isinstance(o,dict):
        for k,v in o.items():
            if k.startswith('_') and k!='_order': continue
            if isinstance(v,dict):
                if '_order' in v: walk(v,path)  # named children
                else: walk(v,path+(k,))
            else: c[(path[-1] if path else '',k)]+=1
for f in sys.argv[1:]:
    j=json.load(open(f))
    for t,v in j.items():
        if t.startswith('_') and len(t)==4: walk(v,())
for (p,k),n in sorted(c.items()): print(p,k,n)
