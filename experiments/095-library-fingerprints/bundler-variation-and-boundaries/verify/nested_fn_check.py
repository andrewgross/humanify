"""VERIFY: variation.py's function match is token-weighted over ALL functions incl. nested ones, so an inner
function's tokens count once per enclosing function too. Re-measure with (a) every function (as published) and
(b) MAXIMAL functions only (no function nested in another listed function), (c) count-weighted maximal.
Pairing exactly as variation.py: same (app, mode, pkg, ver, file), library kinds only, rollup purity >= 0.9.
usage: python3 verify/nested_fn_check.py"""
import json, collections, glob, os, sys
from multiprocessing import Pool
UNITS = '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/data/units'
def load(b):
    labs = json.load(open(f'{UNITS}/{b}.labels.json')); mods = {}
    _, _, app, mode = b.split('/')
    for l in open(f'{UNITS}/{b}.jsonl'):
        r = json.loads(l); lab = labs.get(str(r['lab']))
        if lab is None or r['w']: continue
        pkg, ver, file, kind, marker, purity, sc, ec = lab
        if pkg.startswith('<') or kind not in ('cjs', 'lazy-esm', 'eager-esm'): continue
        m = mods.setdefault((app, mode, pkg, ver, file), {'kind': kind, 'purity': 1.0, 'all': [], 'max': []})
        if purity is not None: m['purity'] = min(m['purity'], purity)
        fs = sorted(r['fns'], key=lambda f: (f[0], -f[1]))
        end = -1
        for f in fs:
            m['all'].append((f[2], f[5], f[6]))
            if f[0] >= end:
                m['max'].append((f[2], f[5], f[6])); end = f[1]
    return b, mods
def contain(a, b, li, count=False):
    cb = collections.Counter(x[li] for x in b); hit = tot = 0
    for x in a:
        w = 1 if count else x[0]; tot += w
        if cb[x[li]] > 0: cb[x[li]] -= 1; hit += w
    return hit, tot
bs = [os.path.relpath(p, UNITS)[:-6] for p in glob.glob(UNITS + '/*/min/*/*.jsonl') if '/cc/' not in p and '/exactver/' not in p]
with Pool(48) as p: data = dict(p.map(load, bs, chunksize=4))
by = collections.defaultdict(dict)
for b, mods in data.items(): by[b.split('/')[0]].update(mods)
res = {}
def run(name, pairs):
    acc = collections.defaultdict(lambda: [0, 0])
    for A, B in pairs:
        for lv, li in (('h3', 1), ('h4', 2)):
            for mode, src, cnt in (('all-tokenW', 'all', False), ('maximal-tokenW', 'max', False), ('maximal-countW', 'max', True)):
                h, t = contain(A[src], B[src], li, cnt); acc[f'{lv} {mode}'][0] += h; acc[f'{lv} {mode}'][1] += t
    res[name] = {k: round(100 * v[0] / v[1], 1) for k, v in acc.items()}; print(name, json.dumps(res[name]), flush=True)
for a, b in (('bun-1.3.14', 'bun-1.4.0'), ('bun-1.4.0', 'bun-1.4.3'), ('bun-1.4.0', 'esbuild-0.28.2'), ('bun-1.4.0', 'rollup-4.64.5'), ('esbuild-0.28.2', 'rollup-4.64.5')):
    run(f'{a} vs {b}', [(A, by[b][k]) for k, A in by[a].items() if k in by[b] and not ('rollup' in a + b and min(A['purity'], by[b][k]['purity']) < 0.9)])
B = by['bun-1.4.0']
run('bun-1.4.0 mode req vs imp', [(A, B[(k[0], 'imp') + k[2:]]) for k, A in B.items() if k[1] == 'req' and (k[0], 'imp') + k[2:] in B])
json.dump(res, open('/work/exp095-library-fingerprints/bundler-variation-and-boundaries/verify/nested-fn-check.json', 'w'), indent=1)
