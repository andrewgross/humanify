"""Population of CC 2.1.216 src/ leaf statements (raw bundle), same unit model as bundler-variation-and-boundaries/cc_scan.mjs.
src/ = every leaf unit NOT inside a CJS factory: depth-0 non-wrapper statements/declarators + depth-1 statements
inside lazy-ESM init wrappers `var X=b(()=>{...})`. Offsets: s/e are JS char offsets == UTF-8 byte offsets (bundle is ASCII).
Writes data/population.jsonl: idx(unit i), s, e, bytes, d, g (esm wrapper unit index or -1 for hoisted/top), grp (group number)"""
import json
U='/work/exp095-library-fingerprints/bundler-variation-and-boundaries/data/units/cc/min/cc-2.1.216/raw.jsonl'
rows=[json.loads(l) for l in open(U)]
wk={r['i']:r['wk'] for r in rows if r['w']}
out=[]; grp=0; pend=[]
cjs_bytes=0; esm_wrap_overhead=0
for r in rows:
    if r['d']==0 and r['w']:
        for u in pend: u['grp']=grp
        pend=[]
        if r['wk']=='cjs': cjs_bytes+=r['e']-r['s']
        r['_grp']=grp; grp+=1
        continue
    if r['d']==1 and wk.get(r['p'])=='cjs': continue
    u={'i':r['i'],'s':r['s'],'e':r['e'],'bytes':r['e']-r['s'],'d':r['d'],'wrap':r['p'] if r['d']==1 else -1,'n':r['n']}
    if r['d']==0: pend.append(u)
    out.append(u)
for u in pend: u['grp']=None
# groups for depth1: wrapper's group
gof={r['i']:r.get('_grp') for r in rows if r['d']==0 and r['w']}
for u in out:
    if u['d']==1: u['grp']=gof[u['wrap']]
with open('/work/exp095-library-fingerprints/negatives/data/population.jsonl','w') as f:
    for u in out: f.write(json.dumps(u)+'\n')
import statistics
tot=sum(u['bytes'] for u in out)
print('units',len(out),'bytes',tot,'cjs factory bytes',cjs_bytes)
for lo,hi in [(0,100),(100,1000),(1000,10000),(10000,10**9)]:
    s=[u for u in out if lo<=u['bytes']<hi]; print(lo,hi,len(s),sum(u['bytes'] for u in s))
