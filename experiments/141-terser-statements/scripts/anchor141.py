"""exp141: terser-made index seed bytes of a carriers.py dossier by verdict, scope and the family's anchor in the
bundle (exclusive chosen-slice seed / only a shared one / none). DEV corpora only.

usage: <venv python> -I anchor141.py <seeds.jsonl> <out.json> <corpus>"""
import collections
import json
import os
import sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../096-library-matching-score"))
import lib096  # noqa: E402
if sys.argv[3] in lib096.PRIVATE:
    raise SystemExit('REFUSED: never a judge corpus')
lib096.use(sys.argv[3])
import score  # noqa: E402
RIGHT=("right","family","origin","selfOrigin")
seeds=[json.loads(l) for l in open(sys.argv[1]) ]
seeds=[x for x in seeds if x['set'].endswith('terser')]
def g(x):
    p=x['package'];a=x['scores'].get(p);b=x['scores0'].get(p); return a is not None and (b is None or a[0]>b[0])
reps=collections.defaultdict(list)
def fam(e,p):
    for q in reps[e]:
        if score.same_family(p,q): return q
    reps[e].append(p); return p
anc=collections.defaultdict(set); anc_any=collections.defaultdict(set)
for x in seeds:
    k=fam(x['entry'],x['package'])
    for d in x['declared']: anc[x['entry']].add(fam(x['entry'],d)); anc_any[x['entry']].add(fam(x['entry'],d))
    if not g(x):
        anc_any[x['entry']].add(k)
        if all(fam(x['entry'],p)==k for p in x['scores']): anc[x['entry']].add(k)
T=collections.Counter()
for x in seeds:
    if not g(x): continue
    k=fam(x['entry'],x['package'])
    st='excl-anchor' if k in anc[x['entry']] else 'shared-anchor-only' if k in anc_any[x['entry']] else 'no-anchor'
    v='R' if x['verdict'] in RIGHT else 'W' if x['verdict']=='wrongFamily' else 'A' if x['verdict']=='app' else 'O'
    T[(x['scope'],st,v)]+=x['bytes']
for k in sorted(T): print(k, round(T[k]/1024,1))
json.dump({'/'.join(k):v for k,v in T.items()}, open(sys.argv[2],'w'), indent=1, sort_keys=True)
