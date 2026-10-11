"""Pooled index (lab + fresh sample, Bun minified only): cross-package shape collisions by size,
split into 'strings differ too' (normV differs = a real false match that normV would catch) and
'strings agree' (normV equal). Shows how collisions grow when the index grows ~3x."""
import json, sys, collections
sys.path.insert(0, 'scripts')
from load import load, is_lib
F = 'verify/fresh'
ent = []  # (pkg, file, normM, normV, normLen)
for r in load():
    if r['ok'] and is_lib(r) and r['file'] and r['cfg'] in ('bun-1.4.0/min',) and (r['purity'] is None or r['purity'] >= 0.9) and r['kind'] in ('cjs','lazy-esm','eager-esm'):
        ent.append(('lab', r['pkg'], r['ver'], r['file'], r['normM'], r['normV'], r['normLen']))
hs = {json.loads(l)['id']: json.loads(l) for l in open(f'{F}/hash.jsonl')}
for i, l in enumerate(open(f'{F}/labels-all.jsonl')):
    r = json.loads(l)
    if r['bundlerVersion'] != '1.4.0' or r['pkg'].startswith('<') or not r['file'] or r['kind'] not in ('cjs','lazy-esm','eager-esm'): continue
    if r['purity'] is not None and r['purity'] < 0.9: continue
    h = hs.get(i)
    if h and h['ok']: ent.append(('fresh', r['pkg'], r['pkgVersion'], r['file'], h['normM'], h['normV'], h['normLen']))
def report(name, E):
    by = collections.defaultdict(set); byV = collections.defaultdict(set); ln = {}
    for src, p, v, f, m, vv, nl in E:
        by[m].add(p); byV[(m, vv)].add(p); ln[m] = nl
    pk = len({e[1] for e in E})
    print(f'## {name}: {len(by)} distinct normM hashes, {pk} packages')
    for lo, hi in [(200,400),(400,600),(600,800),(800,1600),(1600,10**9)]:
        H = [m for m in by if lo <= ln[m] < hi]
        x = [m for m in H if len(by[m]) > 1]
        # does the collision survive normV? i.e. two packages share BOTH shape and strings
        xv = [m for m in x if any(len(ps) > 1 for (mm, vv), ps in byV.items() if mm == m)]
        print(f'   normLen {lo}-{hi}: {len(H)} hashes, cross-package {len(x)} ({100*len(x)/max(1,len(H)):.2f}%), surviving normV {len(xv)}')
    return by
lab_only = [e for e in ent if e[0] == 'lab']
report('lab only (bun-1.4.0/min)', lab_only)
by = report('lab + fresh 400 (bun-1.4.0/min)', ent)
# list the >=400 cross-package collisions in the pool
print('\ncross-package collisions at normLen>=400 in the pooled index:')
ex = collections.defaultdict(set)
for src, p, v, f, m, vv, nl in ent:
    if nl >= 400 and len(by[m]) > 1: ex[(nl, m)].add(f'{p}/{f}')
for (nl, m), s in sorted(ex.items()):
    print(f'  {nl:6d}  ' + ' | '.join(sorted(s)[:4]) + (f' (+{len(s)-4})' if len(s) > 4 else ''))
