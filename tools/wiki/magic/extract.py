import json,sys,os
src,dst=sys.argv[1],sys.argv[2]
d=json.load(open(src))['query']['pages']
for p in d.values():
    t=p['revisions'][0]['slots']['main']['*']
    open(dst,'w').write(t)
    try:
        j=json.loads(t); print(dst,'JSON ok',list(j.keys())[:20])
    except Exception as e: print(dst,'not json',t[:300])
