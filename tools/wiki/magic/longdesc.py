import json,sys,re,html
t=json.load(open(sys.argv[1]))['parse']['text']['*']
t=re.sub(r'<style.*?</style>','',t,flags=re.S)
t=re.sub(r'<[^>]+>',' ',t); t=html.unescape(t); t=re.sub(r'[ \t]+',' ',t); t=re.sub(r'\n\s*\n+','\n',t)
i=t.find('Upgrades [ edit'); j=t.find('Stats [ edit')
if j<0: j=t.find('Costs and sell values [')
up=t[i:j]
for name in sys.argv[2:]:
    k=up.find(name+' Tier')
    if k<0: print('NOT FOUND',name); continue
    m=re.search(r'\n \S[^\n]{0,60} Tier \d', up[k+10:])
    end=k+10+m.start() if m else k+3000
    print('=== ',up[k:end][:2500].replace('\n',' ')); print()
