import sys,re,json
L=open(sys.argv[1]).read().split('\n')
# find Upgrades section start
i=next(k for k,l in enumerate(L) if l.strip()=='Upgrades' and L[k+1].startswith('[edit'))
j=next(k for k in range(i,len(L)) if L[k].strip()=='Stats' and L[k+1].startswith('[edit'))
L=L[i:j]
paths={};cur=None;u=None
k=0
while k<len(L):
    l=L[k].strip()
    m=re.match(r'Path (\d)$',l)
    if m: cur=int(m.group(1)); paths[cur]=[]; k+=1; continue
    if k+1<len(L) and re.match(r'Tier (\d) — ',L[k+1]):
        u={'name':l,'tier':int(L[k+1].split()[1]),'lines':[]}; paths[cur].append(u); k+=2; continue
    if u: u['lines'].append(l)
    k+=1
out={}
for p,us in paths.items():
    for u in us:
        ls=[x for x in u['lines'] if not x.startswith('[img:Icon')]
        med=[x for x in ls if x.endswith('— Medium')]
        ci=ls.index(med[0]) if med else None
        u['cost']=int(re.sub(r'[^\d]','',med[0].split('—')[0])) if med else None
        u['short']=' '.join(ls[:ci-1]) if ci else ''
        u['detail']=' '.join(ls[ci+3:]) if ci else ' '.join(ls)
        del u['lines']
    out[p]=us
json.dump(out,open(sys.argv[2],'w'),indent=1,ensure_ascii=False)
for p,us in out.items():
    for u in us: print(p,u['tier'],u['name'],u['cost'],'|',u['short'][:80])
