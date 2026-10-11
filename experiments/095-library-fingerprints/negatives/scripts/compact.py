import re,sys
t=re.split(r'\n(?=### id=\d+ stratum=)', open(sys.argv[1]).read())
for x in t:
    lines=x.split('\n'); out=[]; tok=0; mode=None
    for l in lines:
        if l.startswith('TOKENS'): mode='tok'; continue
        if l.startswith('NEIGHBOURS'): mode='nb'; continue
        if mode=='tok' and l.startswith('  '):
            tok+=1
            if tok<=7: out.append(l[:140])
            continue
        if mode=='nb' and l.startswith('  '):
            out.append(l[:170]); continue
        mode=None
        if l.startswith(('DEP GROUPS','CALLER GROUPS')): out.append(l[:160]); continue
        if l.startswith('GROUP VOTES'): out.append(l[:260]); continue
        if l.startswith('TEXT'): out.append(l[:int(sys.argv[2]) if len(sys.argv)>2 else 700]); continue
        out.append(l[:400])
    print('\n'.join(out)); print()
