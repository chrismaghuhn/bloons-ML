import json,sys;r=json.load(open(sys.argv[1]))
print(json.dumps({k:v for k,v in r.items() if k not in('upgrades','source_urls')},indent=0))
for p,l in r['upgrades'].items():
  for e in l: print(p,e['tier'],e['name'],e['cost']); print('   ',json.dumps(e['changes']))
