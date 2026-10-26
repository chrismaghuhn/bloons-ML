import json,sys,re,html
from html.parser import HTMLParser
class P(HTMLParser):
    def __init__(s):
        super().__init__(); s.out=[]; s.skip=0
    def handle_starttag(s,t,a):
        if t in('style','script'): s.skip+=1
        if t in('br','p','div','li','h2','h3','h4','table','dd','dt','ul'): s.out.append('\n')
        if t=='tr': s.out.append('\n| ')
        if t in('td','th'): s.out.append(' | ')
        if t=='img':
            d=dict(a)
            if d.get('alt'): s.out.append('[img:%s]'%d['alt'])
    def handle_endtag(s,t):
        if t in('style','script'): s.skip-=1
        if t in('h2','h3','h4'): s.out.append('\n')
    def handle_data(s,d):
        if not s.skip: s.out.append(d)
h=json.load(open(sys.argv[1]))['parse']['text']['*']
p=P(); p.feed(h)
t=''.join(p.out)
t=re.sub(r'[ \t]+',' ',t); t=re.sub(r'\n\s*\n+','\n',t)
open(sys.argv[2],'w').write(t)
