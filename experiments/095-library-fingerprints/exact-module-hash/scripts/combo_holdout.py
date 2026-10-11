"""HOLD-OUT: index built from the single-package apps only; test on the 6 combo apps (8-9 libraries each,
app code interleaved, every library reached its own way) - the closest the lab gets to a real bundle.
Recall (spans and bytes) and precision (does the candidate set contain the TRUE file / package?).
Output: results/combo_holdout.json + printed table."""
import json, collections, sys
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib, HERE

rows = [r for r in load() if r['ok'] and (r['purity'] is None or r['purity'] >= 0.9)]
LENKEY = {'coreM': 'coreLen', 'normM': 'normLen', 'normV': 'normLen'}
out = {}
for test_cfg, idx_pred, label in [
    ('bun-1.4.0/min', lambda c: c == 'bun-1.4.0/min', 'index bun-1.4.0/min'),
    ('bun-1.4.0/min', lambda c: c.startswith('bun') and c.endswith('/min'), 'index bun all min'),
    ('bun-1.4.0/min', lambda c: c in ('bun-1.3.14/min', 'bun-1.4.3/min'), 'index OTHER bun versions only'),
    ('bun-1.4.0/min', lambda c: c == 'esbuild-0.28.2/min', 'index esbuild only'),
    ('esbuild-0.28.2/min', lambda c: c == 'esbuild-0.28.2/min', 'esbuild index -> esbuild combos'),
]:
    train = [r for r in rows if idx_pred(r['cfg']) and not r['app'].startswith('combo-') and is_lib(r) and r['file']]
    test = [r for r in rows if r['cfg'] == test_cfg and r['app'].startswith('combo-')]
    for v in ['coreM', 'normM', 'normV']:
        idx = collections.defaultdict(set)
        for r in train:
            idx[r[v]].add((r['pkg'], r['file']))
        for floor in [3, 400, 800]:
            st = collections.Counter()
            for r in test:
                lib = is_lib(r) and r['file']
                big = r[LENKEY[v]] >= floor
                hit = big and r[v] in idx
                if lib:
                    st['libSpans'] += 1
                    st['libBytes'] += r['bytes']
                    st[f"lib:{r['kind']}"] += 1
                    if hit:
                        cands = idx[r[v]]
                        st['hit'] += 1
                        st['hitBytes'] += r['bytes']
                        st[f"hit:{r['kind']}"] += 1
                        if (r['pkg'], r['file']) in cands:
                            st['trueFileInCands'] += 1
                            if len(cands) == 1:
                                st['uniqueTrueFile'] += 1
                        elif r['pkg'] in {p for p, _ in cands}:
                            st['rightPkgWrongFile'] += 1
                        else:
                            st['wrongPkg'] += 1
                elif r['pkg'] == '<app>':
                    st['appSpans'] += 1
                    st['appHits'] += hit
            out[f'{label}|{v}|{floor}'] = dict(st)
            if v == 'normV' and floor != 400:
                continue
            print(f"{label:32s} {v:5s} len>={floor:<4d} recall {st['hit']}/{st['libSpans']} spans "
                  f"({100*st['hit']/st['libSpans']:.1f}%), {100*st['hitBytes']/st['libBytes']:.1f}% of bytes | "
                  f"cjs {st['hit:cjs']}/{st['lib:cjs']} lazy {st['hit:lazy-esm']}/{st['lib:lazy-esm']} eager {st['hit:eager-esm']}/{st['lib:eager-esm']} | "
                  f"true file in cands {st['trueFileInCands']} (unique {st['uniqueTrueFile']}), right pkg wrong file {st['rightPkgWrongFile']}, WRONG PKG {st['wrongPkg']} | app hits {st['appHits']}/{st['appSpans']}")
json.dump(out, open(f'{HERE}/results/combo_holdout.json', 'w'), indent=1)
