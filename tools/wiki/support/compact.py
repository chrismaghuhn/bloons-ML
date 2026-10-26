import sys,re
L=open(sys.argv[1]).read().split('\n')
out=[];buf=''
skip=('[edit','| | ','')
i=0
lines=[]
# join continuation lines (lines not starting with '|' after a '| | key | ' line)
for l in L:
    if l.startswith('Navigation'): break
    if l.startswith('|') or not lines: lines.append(l)
    elif re.match(r'(Tier \d|Path \d|Base stats|\d-\d-\d)$',l.strip()): lines.append('##'+l.strip())
    elif l.startswith('[edit'): continue
    else: lines[-1]+=' '+l.strip()
for l in lines:
    if l.startswith('##'): print('\n'+l); continue
    parts=[p.strip() for p in l.split('|') if p.strip()]
    if not parts: continue
    if len(parts)==1: print('\n  ['+parts[0]+']',end=' ')
    else: print(parts[0]+'='+' '.join(parts[1:]),end='; ')
print()
