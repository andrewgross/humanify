"""Run the Step B experiments on lab bundles. usage: python3 scripts/stepb_run.py [quick]
Writes results/stepb-lab.json and prints tables."""
import json, sys, collections, os, glob
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, index_from_builds, detect, score, summarize, ROOT, UNITS, NONLIB
from load import build_dirs

ALL = build_dirs()
mins = [b for b in ALL if '/min/' in b]
singles = lambda cfgs: [b for b in mins if b.split('/')[0] in cfgs and not b.split('/')[2].startswith('combo')]
combos = lambda cfg: [b for b in mins if b.startswith(cfg + '/') and b.split('/')[2].startswith('combo')]

INDEXES = {
    'other-bundlers (bun-1.3.14+bun-1.4.3+esbuild)': (singles({'bun-1.3.14', 'bun-1.4.3', 'esbuild-0.28.2'}), 'h3'),
    'same bundler (bun-1.4.0)': (singles({'bun-1.4.0'}), 'h3'),
    'esbuild only': (singles({'esbuild-0.28.2'}), 'h3'),
    'rollup+terser only (masked h4)': (singles({'rollup-4.64.5'}), 'h4'),
}
QUERIES = {'bun-1.4.0 combos': combos('bun-1.4.0'), 'esbuild combos': combos('esbuild-0.28.2')}

res = {}
qunits = {q: {b: load_units(b) for b in bs} for q, bs in QUERIES.items()}
for iname, (ibuilds, level) in INDEXES.items():
    idx = index_from_builds(ibuilds, level)
    print(f'\n### index: {iname}  builds={len(ibuilds)} fn-hashes={len(idx["fn"])} stmt-hashes={len(idx["stmt"])} str={len(idx["str"])}', flush=True)
    for qname, ub in qunits.items():
        if 'esbuild combos' == qname and iname != 'same bundler (bun-1.4.0)' and iname != 'other-bundlers (bun-1.3.14+bun-1.4.3+esbuild)':
            continue
        # package pairs that demonstrably contain copies of each other's code: >= 3 shared functions of >= 100 tokens
        pair_shared = collections.Counter()
        for h, ps in idx['fn'].items():
            if idx['fnN'].get(h, 0) >= 100 and 1 < len(ps) <= 6 and '<nonlib>' not in ps:
                for a in ps:
                    for b2 in ps:
                        if a < b2:
                            pair_shared[(a, b2)] += 1
        shares = lambda a, b2: pair_shared[(min(a, b2), max(a, b2))] >= 3
        for strategy, mc in (('S1', 3), ('S2', 3), ('S3', 0), ('S3', 3), ('S3', 5), ('S4', 3), ('A1', 0), ('A3', 0)):
            for nmin in ((12, 20, 30, 50, 80) if (strategy, mc) == ('S3', 3) else (30,)):
                tot = collections.Counter()
                loo = collections.Counter()
                for b, units in ub.items():
                    pred = detect(units, idx, nmin=nmin, strategy=strategy, mincontent=mc)
                    tot += score(units, pred)
                    # leave-one-package-out: each package's own bytes when it is NOT in the index
                    pkgs = {u['truth'][0] for u in units if u['truth'][0] not in NONLIB and u['truth'][0] != '<none>'}
                    for P in pkgs:
                        pr = detect(units, idx, nmin=nmin, strategy=strategy, exclude=P, mincontent=mc)
                        for u, p in zip(units, pr):
                            if u['truth'][0] == P:
                                loo['bytes'] += u['e'] - u['s']
                                if p:
                                    loo['flagged'] += u['e'] - u['s']
                                    loo['flaggedShared' if shares(p, P) else 'flaggedCoincidental'] += u['e'] - u['s']
                                    loo[f'flaggedAs:{p}<-{P}'] += u['e'] - u['s']
                s = summarize(tot)
                s['LOO unknown-code flagged%'] = round(100 * loo['flagged'] / loo['bytes'], 2) if loo['bytes'] else None
                s['LOO flagged, shared-code pair%'] = round(100 * loo['flaggedShared'] / loo['bytes'], 2) if loo['bytes'] else None
                s['LOO flagged, coincidental%'] = round(100 * loo['flaggedCoincidental'] / loo['bytes'], 2) if loo['bytes'] else None
                s['LOO MB'] = round(loo['bytes'] / 1e6, 2)
                s['LOO top confusions'] = [(k.split(':', 1)[1], v) for k, v in sorted(((k, v) for k, v in loo.items() if k.startswith('flaggedAs')), key=lambda x: -x[1])[:6]]
                key = f'{iname} | {qname} | {strategy} nmin={nmin} mincontent={mc}'
                res[key] = s
                print(key, json.dumps(s), flush=True)
json.dump(res, open(ROOT + '/results/stepb-lab.json', 'w'), indent=1)
