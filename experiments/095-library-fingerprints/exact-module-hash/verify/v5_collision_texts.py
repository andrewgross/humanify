"""Dump one text per (package,file) for every cross-package normM collision at normLen>=400 in the pooled
(lab + fresh, bun-1.4.0/min) index, skipping core-js vs core-js-pure. Output verify/v5_collision_texts.txt"""
import json, sys, collections
sys.path.insert(0, 'scripts')
from load import load, is_lib
F = 'verify/fresh'
ent = collections.defaultdict(dict)
def add(p, f, h, t, nl, v):
    ent[h].setdefault((p, f), (t, nl, v))
for r in load():
    if r['ok'] and is_lib(r) and r['file'] and r['cfg'] == 'bun-1.4.0/min' and (r['purity'] is None or r['purity'] >= 0.9) and r['kind'] in ('cjs','lazy-esm','eager-esm'):
        add(r['pkg'], r['file'], r['normM'], '/work/exp095-library-fingerprints/lab/' + r['textPath'], r['normLen'], r['normV'])
hs = {json.loads(l)['id']: json.loads(l) for l in open(f'{F}/hash.jsonl')}
for i, l in enumerate(open(f'{F}/labels-all.jsonl')):
    r = json.loads(l)
    if r['bundlerVersion'] != '1.4.0' or r['pkg'].startswith('<') or not r['file'] or r['kind'] not in ('cjs','lazy-esm','eager-esm'): continue
    if r['purity'] is not None and r['purity'] < 0.9: continue
    h = hs.get(i)
    if h and h['ok']: add(r['pkg'], r['file'], h['normM'], f"{F}/{r['textPath']}", h['normLen'], h['normV'])
out = open('verify/v5_collision_texts.txt', 'w')
n = 0
for h, d in sorted(ent.items(), key=lambda x: min(v[1] for v in x[1].values())):
    pk = {p for p, f in d}
    if len(pk) < 2 or pk <= {'core-js', 'core-js-pure'}: continue
    nl = min(v[1] for v in d.values())
    if nl < 400: continue
    n += 1
    out.write(f'\n##### normM {h} normLen {nl} packages {sorted(pk)} normV-distinct {len({v[2] for v in d.values()})}\n')
    for (p, f), (t, _, v) in sorted(d.items())[:6]:
        out.write(f'--- {p}/{f} normV {v}\n{open(t).read()[:400]}\n')
print(n, 'groups')
