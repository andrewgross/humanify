"""STABILITY: is the same library file given the same hash under a different bundler / version /
minification / entry mode? Output: results/stability.json + printed tables."""
import json, collections, sys
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib, VARIANTS, HERE

rows = [r for r in load() if is_lib(r) and r['ok'] and r['parts'] == 1]


def ok_purity(r):
    return r['purity'] is None or r['purity'] >= 0.9


by_cfg = collections.defaultdict(dict)
dups = collections.Counter()
for r in rows:
    if not ok_purity(r):
        continue
    k = (r['app'], r['mode'], r['pkg'], r['ver'], r['file'])
    if k in by_cfg[r['cfg']]:
        dups[r['cfg']] += 1
        by_cfg[r['cfg']][k] = None  # ambiguous: drop
        continue
    by_cfg[r['cfg']][k] = r

PAIRS = [
    ('bun-1.4.0/min', 'bun-1.4.3/min', 'Bun 1.4.0 vs 1.4.3 (patch bump)'),
    ('bun-1.3.14/min', 'bun-1.4.0/min', 'Bun 1.3.14 vs 1.4.0 (minor bump)'),
    ('bun-1.4.0/min', 'esbuild-0.28.2/min', 'Bun 1.4.0 vs esbuild (both minified)'),
    ('bun-1.4.0/min', 'rollup-4.64.5/min', 'Bun 1.4.0 vs rollup+terser'),
    ('esbuild-0.28.2/min', 'rollup-4.64.5/min', 'esbuild vs rollup+terser'),
    ('bun-1.4.0/min', 'bun-1.4.0/unmin', 'Bun 1.4.0 minified vs not'),
    ('esbuild-0.28.2/min', 'esbuild-0.28.2/unmin', 'esbuild minified vs not'),
    ('bun-1.4.0/unmin', 'esbuild-0.28.2/unmin', 'Bun 1.4.0 vs esbuild (both unminified)'),
    ('bun-1.4.0/unmin', 'bun-1.4.3/unmin', 'Bun 1.4.0 vs 1.4.3 unminified'),
]


def compare(a, b):
    A, B = by_cfg[a], by_cfg[b]
    keys = [k for k in A if A[k] is not None and B.get(k) is not None]
    res = {'n': len(keys), 'variants': {}, 'byKind': {}}
    for v in VARIANTS:
        res['variants'][v] = sum(1 for k in keys if A[k][v] == B[k][v])
    kinds = collections.defaultdict(list)
    for k in keys:
        kinds[f"{A[k]['kind']}->{B[k]['kind']}"].append(k)
    for kk, ks in sorted(kinds.items(), key=lambda x: -len(x[1])):
        res['byKind'][kk] = {'n': len(ks), **{v: sum(1 for k in ks if A[k][v] == B[k][v]) for v in VARIANTS}}
    return res


out = {'dupsDropped': dict(dups), 'pairs': {}, 'modes': {}}
print('dup keys dropped per config', dict(dups))
SHOW = ['raw', 'fullM', 'fullV', 'coreM', 'coreSetM', 'normStmt', 'normM', 'normV', 'normSetM']
print(f"{'pair':45s} {'n':>6s} " + ' '.join(f'{v:>9s}' for v in SHOW))
for a, b, name in PAIRS:
    res = compare(a, b)
    out['pairs'][name] = {'a': a, 'b': b, **res}
    n = res['n']
    print(f"{name:45s} {n:6d} " + ' '.join(f"{100*res['variants'][v]/max(n,1):8.1f}%" for v in SHOW))
    for kk, d in list(res['byKind'].items())[:4]:
        print(f"   {kk:42s} {d['n']:6d} " + ' '.join(f"{100*d[v]/max(d['n'],1):8.1f}%" for v in SHOW))

# entry MODE (how the app reaches the library) within one config: wrapper differences
print('\nSame config, same app, different entry mode')
MODEP = [('req', 'imp'), ('imp', 'dyn'), ('req', 'dyn'), ('imp', 'some')]
for cfg in ['bun-1.4.0/min', 'esbuild-0.28.2/min', 'rollup-4.64.5/min', 'bun-1.4.0/unmin']:
    A = by_cfg[cfg]
    idx = collections.defaultdict(dict)
    for k, r in A.items():
        if r is None:
            continue
        app, mode, pkg, ver, f = k
        idx[(app, pkg, ver, f)][mode] = r
    for m1, m2 in MODEP:
        pairs = [(d[m1], d[m2]) for d in idx.values() if m1 in d and m2 in d]
        kinds = collections.defaultdict(list)
        for x, y in pairs:
            kinds[f"{x['kind']}->{y['kind']}"].append((x, y))
        res = {'n': len(pairs), **{v: sum(1 for x, y in pairs if x[v] == y[v]) for v in VARIANTS},
               'byKind': {kk: {'n': len(ps), **{v: sum(1 for x, y in ps if x[v] == y[v]) for v in VARIANTS}}
                          for kk, ps in kinds.items()}}
        out['modes'][f'{cfg} {m1}-{m2}'] = res
        n = res['n']
        print(f"{cfg+' '+m1+' vs '+m2:45s} {n:6d} " + ' '.join(f"{100*res[v]/max(n,1):8.1f}%" for v in SHOW))
        for kk, d in sorted(res['byKind'].items(), key=lambda x: -x[1]['n'])[:4]:
            print(f"   {kk:42s} {d['n']:6d} " + ' '.join(f"{100*d[v]/max(d['n'],1):8.1f}%" for v in SHOW))

json.dump(out, open(f'{HERE}/results/stability.json', 'w'), indent=1)
