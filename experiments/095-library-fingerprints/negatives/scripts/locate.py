"""locate.py <regex>... : print population units (src leaf statements) whose start matches each regex's first match location"""
import re,sys,json,bisect
B='/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js'
s=open(B).read()
pop=sorted((json.loads(l) for l in open('/work/exp095-library-fingerprints/negatives/data/population.jsonl')),key=lambda u:u['s'])
ss=[u['s'] for u in pop]
for pat in sys.argv[1:]:
    for m in re.finditer(pat,s):
        p=m.start(); k=bisect.bisect_right(ss,p)-1
        u=pop[k] if k>=0 and pop[k]['s']<=p<pop[k]['e'] else None
        if u: print(json.dumps({'pat':pat,'unit':u['i'],'start':u['s'],'end':u['e'],'bytes':u['e']-u['s'],'grp':u['grp'],'head':s[u['s']:u['s']+120]}))
        else: print(json.dumps({'pat':pat,'at':p,'unit':None,'head':s[p:p+100]}))
