"""Final configuration comparison (lab combos + leave-one-out, and Claude Code negatives/positives/census).
usage: PYTHONHASHSEED=0 python3 scripts/stepb_final.py -> results/stepb-final.json"""
import json, sys, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, index_from_builds, detect, score, summarize, NONLIB, ROOT, UNITS
from load import build_dirs

mins = [b for b in build_dirs() if '/min/' in b and not b.startswith('cc/')]
singles = lambda cfgs: [b for b in mins if b.split('/')[0] in cfgs and not b.split('/')[2].startswith('combo')]
CONFIGS = [
    ('S3 nmin=30 content>=0', dict(strategy='S3', nmin=30, mincontent=0)),
    ('S3 nmin=30 content>=3', dict(strategy='S3', nmin=30, mincontent=3)),
    ('S3 nmin=30 content>=3 support=10', dict(strategy='S3', nmin=30, mincontent=3, support=10)),
    ('S3 nmin=30 content>=5 support=10', dict(strategy='S3', nmin=30, mincontent=5, support=10)),
    ('S3 nmin=30 content>=3 support=10 unique-pkg', dict(strategy='S3', nmin=30, mincontent=3, support=10, maxpk=1)),
    ('S3 nmin=30 stmt-content>=3 fn-content>=0 support=10', dict(strategy='S3', nmin=30, mincontent=3, minfncontent=0, support=10)),
    ('S3 nmin=30 stmt-content>=3 fn-content>=2 support=10', dict(strategy='S3', nmin=30, mincontent=3, minfncontent=2, support=10)),
]
LABHLJS = {'javascript', 'python', 'bash', 'json', 'html, xml', 'xml', 'css', 'typescript', 'go', 'rust', 'sql', 'c++', 'c', 'actionscript', 'arduino'}
res = {}

# lab: index from other bundlers' single-package apps; query Bun 1.4.0 and esbuild combos
idx = index_from_builds(singles({'bun-1.3.14', 'bun-1.4.3', 'esbuild-0.28.2'}), 'h3')
pair_shared = collections.Counter()
for h, ps in idx['fn'].items():
    if idx['fnN'].get(h, 0) >= 100 and 1 < len(ps) <= 6 and '<nonlib>' not in ps:
        for a in ps:
            for b2 in ps:
                if a < b2:
                    pair_shared[(a, b2)] += 1
shares = lambda a, b2: pair_shared[(min(a, b2), max(a, b2))] >= 3
for qcfg in ('bun-1.4.0', 'esbuild-0.28.2'):
    qb = {b: load_units(b) for b in mins if b.startswith(qcfg + '/') and b.split('/')[2].startswith('combo')}
    for name, kw in CONFIGS:
        tot = collections.Counter()
        loo = collections.Counter()
        for b, units in qb.items():
            tot += score(units, detect(units, idx, **kw))
            for P in {u['truth'][0] for u in units if u['truth'][0] not in NONLIB and u['truth'][0] != '<none>'}:
                for u, p in zip(units, detect(units, idx, exclude=P, **kw)):
                    if u['truth'][0] == P:
                        loo['bytes'] += u['e'] - u['s']
                        if p:
                            loo['shared' if shares(p, P) else 'coinc'] += u['e'] - u['s']
        s = summarize(tot)
        s['LOO shared-code%'] = round(100 * loo['shared'] / loo['bytes'], 2)
        s['LOO coincidental%'] = round(100 * loo['coinc'] / loo['bytes'], 2)
        s['LOO coincidental KB'] = round(loo['coinc'] / 1e3, 1)
        s['LOO MB'] = round(loo['bytes'] / 1e6, 2)
        key = f'lab | {qcfg} combos | {name}'
        res[key] = s
        print(key, json.dumps(s), flush=True)

# Claude Code: index from the whole lab
idx = index_from_builds(mins, 'h3')
idx_pkgs = set().union(*idx['fn'].values())
for ver in ('2.1.216', '2.1.198'):
    units = load_units(f'cc/min/cc-{ver}/raw')
    rows = [json.loads(l) for l in open(f'{UNITS}/cc/min/cc-{ver}/raw.jsonl')]
    wk = {r['i']: r['wk'] for r in rows if r['w']}
    insrc = set()
    pend = []
    for r in rows:
        if r['w']:
            if wk[r['i']] == 'esm':
                insrc.update(pend)
            pend = []
        elif r['d'] == 1:
            if wk[r['p']] == 'esm':
                insrc.add(r['s'])
        else:
            pend.append(r['s'])
    for name, kw in CONFIGS:
        pred = detect(units, idx, **kw)
        r = score(units, pred)
        kp = collections.Counter()
        srcb = collections.Counter()
        for u, p in zip(units, pred):
            t = u['truth']
            b = u['e'] - u['s']
            if u['s'] in insrc and t[0] == '<none>':
                srcb['src'] += b
                if p:
                    srcb['flagged'] += b
            if t[0] in NONLIB or t[0] == '<none>':
                continue
            known = ((t[3] or '').lower() in LABHLJS) if t[0] == 'highlight.js' else (t[0] in idx_pkgs)
            k = 'known' if known else 'unknown'
            kp[k] += b
            if p:
                kp[k + 'Found'] += b
                kp[k + 'Right'] += b if p == t[0] else 0
        s = {
            'app negatives flagged bytes': r['nonlibFlagged'],
            'app negatives KB': round(r['nonlibBytes'] / 1e3, 1),
            'known positives recall%': round(100 * kp['knownFound'] / kp['known'], 1),
            'known positives right pkg%': round(100 * kp['knownRight'] / max(1, kp['knownFound']), 1),
            'known positives KB': round(kp['known'] / 1e3, 1),
            'lazy-ESM (src/) bytes labelled library MB': f"{srcb['flagged']/1e6:.2f} of {srcb['src']/1e6:.2f}",
        }
        key = f'cc {ver} | {name}'
        res[key] = s
        print(key, json.dumps(s), flush=True)
json.dump(res, open(ROOT + '/results/stepb-final.json', 'w'), indent=1)
