"""Hold-out test of the normaliser: norm.rs rules were derived from lab diffs. On the 400 FRESH packages,
is the same file given the same hash by Bun 1.3.14 and Bun 1.4.0 (same app, same mode)? Same pairing as stability.py."""
import json, collections
F = 'verify/fresh'
hs = {json.loads(l)['id']: json.loads(l) for l in open(f'{F}/hash.jsonl')}
by = collections.defaultdict(dict)
for i, l in enumerate(open(f'{F}/labels-all.jsonl')):
    r = json.loads(l)
    if r['pkg'].startswith('<') or not r['file'] or r['parts'] != 1 or (r['purity'] is not None and r['purity'] < 0.9): continue
    h = hs.get(i)
    if not h or not h['ok']: continue
    k = (r['app'], r['mode'], r['pkg'], r['pkgVersion'], r['file'])
    d = by[r['bundlerVersion']]
    d[k] = None if k in d else (r, h)
A, B = by['1.3.14'], by['1.4.0']
keys = [k for k in A if A[k] and B.get(k)]
V = ['fullM', 'coreM', 'normM', 'normV']
print('pairs', len(keys))
for v in V:
    print(f'  {v}: {100*sum(A[k][1][v]==B[k][1][v] for k in keys)/len(keys):.1f}% equal')
for kind in ['cjs', 'lazy-esm', 'eager-esm']:
    ks = [k for k in keys if A[k][0]['kind'] == kind]
    print(f'  {kind:9s} n={len(ks):6d} ' + ' '.join(f"{v} {100*sum(A[k][1][v]==B[k][1][v] for k in ks)/max(1,len(ks)):.1f}%" for v in V))
big = [k for k in keys if B[k][1]['normLen'] >= 400]
print(f'  normLen>=400 n={len(big)} normM equal {100*sum(A[k][1]["normM"]==B[k][1]["normM"] for k in big)/len(big):.1f}%')
bad = [k for k in keys if A[k][1]['normM'] != B[k][1]['normM']]
print('  differing examples:', [(k[2], k[4], A[k][0]['kind']) for k in bad[:12]])
json.dump({'n': len(keys), 'bad': [list(k) + [A[k][0]['textPath'], B[k][0]['textPath']] for k in bad]}, open('verify/v7_fresh_stability.json', 'w'), indent=1)
