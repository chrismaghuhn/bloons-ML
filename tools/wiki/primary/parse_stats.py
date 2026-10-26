"""Parse Stats:<Tower> (BTD6) rendered HTML into a tree per tier. Usage: parse_stats.py <stats_html.json> [dump]"""
import json, sys, re, html
from html.parser import HTMLParser

class Node:
    def __init__(self, kind, cls=''):
        self.kind=kind; self.cls=cls; self.children=[]; self.text=''; self.attrs={}
class P(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.root=Node('root'); self.stack=[self.root]
    def handle_starttag(self, tag, attrs):
        a=dict(attrs)
        if tag in ('br','img','hr','meta','link','input'):
            if tag=='br': self.stack[-1].text+=' '
            return
        n=Node(tag, a.get('class','') or ''); n.attrs=a
        self.stack[-1].children.append(n); self.stack.append(n)
    def handle_endtag(self, tag):
        for i in range(len(self.stack)-1,0,-1):
            if self.stack[i].kind==tag:
                del self.stack[i:]; return
    def handle_data(self, d):
        self.stack[-1].children.append(d)

def text(n):
    if isinstance(n,str): return n
    if n.kind=='li': return '; '+''.join(text(c) for c in n.children)
    t=''.join(text(c) for c in n.children)
    if n.kind=='span' and 'explain' in n.cls and n.attrs.get('title'):
        t=t+' ['+n.attrs['title']+']'
    return t
def clean(s): return re.sub(r'\s+',' ',s).strip().lstrip('; ').strip()

def elems(n, kind=None):
    return [c for c in n.children if not isinstance(c,str) and (kind is None or c.kind==kind)]

def parse_stats_table(t):
    """t is a <table class=stats ...>. returns dict {head, props:[(k,v)], children:[...]}"""
    out={'head':None,'props':[],'children':[]}
    tbody=elems(t,'tbody'); body=tbody[0] if tbody else t
    def walk_rows(body):
        for tr in elems(body,'tr'):
            for cell in elems(tr):
                if cell.kind=='th' and 'head' in cell.cls:
                    out['head']=clean(text(cell))
                elif cell.kind=='td':
                    handle_td(cell)
    def handle_td(td):
        for c in elems(td):
            if c.kind=='table' and 'stats-sub' in c.cls:
                b=elems(c,'tbody'); b=b[0] if b else c
                for tr in elems(b,'tr'):
                    th=elems(tr,'th'); tds=elems(tr,'td')
                    if th and tds:
                        out['props'].append((clean(text(th[0])), clean(text(tds[0]))))
                    elif tds:
                        # nested tables inside sub rows
                        for cc in elems(tds[0]):
                            if cc.kind=='table' and 'stats' in cc.cls.split():
                                out['children'].append(parse_stats_table(cc))
            elif c.kind=='table' and 'stats' in c.cls.split():
                out['children'].append(parse_stats_table(c))
            elif c.kind in ('div','span'):
                handle_td(c)
    walk_rows(body)
    return out

def parse(path):
    h=json.load(open(path))['parse']['text']['*']
    p=P(); p.feed(h)
    # walk top-level of content in order; track headings
    res={}; cur_path=None; cur_key=None
    order=[]
    def visit(n):
        nonlocal cur_path, cur_key
        for c in n.children:
            if isinstance(c,str): continue
            if c.kind in ('h2','h3'):
                hid=c.attrs.get('id','')
                if c.kind=='h2':
                    cur_path=hid; 
                    if hid=='Base_stats': cur_key='000'
                    else: cur_key=None
                else:
                    m=re.match(r'Tier_(\d)',hid)
                    if m and cur_path and cur_path.startswith('Path_'):
                        pi=int(cur_path[5]); ti=int(m.group(1))
                        code=['0','0','0']; code[pi-1]=str(ti); cur_key=''.join(code)
                continue
            if c.kind=='table' and 'stats' in c.cls.split():
                if cur_key:
                    res.setdefault(cur_key,[]).append(parse_stats_table(c))
                continue
            visit(c)
    visit(p.root)
    return res

def dump(t, ind=0, f=sys.stdout):
    pad='  '*ind
    print(f"{pad}[{t['head']}]", file=f)
    for k,v in t['props']:
        print(f"{pad}  {k}: {v}", file=f)
    for c in t['children']: dump(c, ind+1, f)

if __name__=='__main__':
    r=parse(sys.argv[1])
    for k in sorted(r, key=lambda x:(x!='000', x[::-1] if False else x)):
        pass
    keys=['000']+[f'{i}00' for i in range(1,6)]+[f'0{i}0' for i in range(1,6)]+[f'00{i}' for i in range(1,6)]
    for k in keys:
        print(f'===== {k} =====')
        for t in r.get(k,[]): dump(t)
