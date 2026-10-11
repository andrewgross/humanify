"""Step B on Claude Code with the EXACT library versions it ships, scored against banner ground truth.

Truth: every MSAL module in Claude Code carries `/*! @azure/msal-common v15.13.1 ...*/` or `/*! @azure/msal-node v3.8.1 */`
inside (or right before) its lazy-ESM init wrapper. A wrapper group (the wrapper plus the hoisted statements before it)
holding such a banner is MSAL code. Those groups are exact positives for a lazy-ESM library with NO module boundary
start. Statements labelled msal OUTSIDE those groups are counted as msal false positives.

Index A = the lab (recent versions: msal-node 6/7, msal-common 16.x, axios 1.19/1.20, zod 3.23/4.x)
Index B = A + exact versions (exactver/: msal-node 3.8.1 + msal-common 15.13.1, axios 1.15.2, zod 3.25.76 incl zod/v4)
usage: PYTHONHASHSEED=0 python3 scripts/cc_exact.py -> results/stepb-cc-exact.json
"""
import json, sys, collections, glob, os
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, index_from_builds, detect, score, NONLIB, ROOT, UNITS
from load import build_dirs

BUNDLE = '/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-{}/binary-decompiled/src/entrypoints/index.js'
labb = [b for b in build_dirs() if '/min/' in b and not b.startswith('cc/')]
exact = sorted(os.path.relpath(p, UNITS)[:-6] for p in glob.glob(UNITS + '/exactver/*/min/*/*.jsonl'))
CONFIG = dict(strategy='S3', nmin=30, mincontent=3, minfncontent=0, support=10)
MSAL = ('@azure/msal-common', '@azure/msal-node')
res = {}
for iname, builds in (('A lab only', labb), ('B lab + exact versions', labb + exact)):
    idx = index_from_builds(builds, 'h3')
    for ver in ('2.1.216', '2.1.198'):
        units = load_units(f'cc/min/cc-{ver}/raw')
        rows = [json.loads(l) for l in open(f'{UNITS}/cc/min/cc-{ver}/raw.jsonl')]
        text16 = open(BUNDLE.format(ver), encoding='utf8').read().encode('utf-16-le')
        # banner positions in JS (UTF-16) offsets
        pos = []
        for needle in ('/*! @azure/msal-common v'.encode('utf-16-le'), '/*! @azure/msal-node v'.encode('utf-16-le')):
            i = text16.find(needle)
            while i >= 0:
                if i % 2 == 0:
                    pos.append(i // 2)
                i = text16.find(needle, i + 2)
        wrappers = [(r['s'], r['e'], r['i']) for r in rows if r['w'] and r['wk'] == 'esm']
        wrappers.sort()
        import bisect
        ws = [w[0] for w in wrappers]
        msal_groups = set()
        gid = {}  # wrapper unit index -> group number (same numbering as load_units)
        g = 0
        for r in rows:
            if r['d'] == 0 and r['w']:
                gid[r['i']] = g
                g += 1
        for p in pos:
            k = bisect.bisect_right(ws, p) - 1
            if k >= 0 and wrappers[k][0] <= p < wrappers[k][1]:
                msal_groups.add(gid[wrappers[k][2]])
            elif k + 1 < len(wrappers):
                msal_groups.add(gid[wrappers[k + 1][2]])  # banner in the hoisted part before the next wrapper
        pred = detect(units, idx, **CONFIG)
        c = collections.Counter()
        bypkg = collections.Counter()
        for u, p in zip(units, pred):
            b = u['e'] - u['s']
            t = u['truth'][0]
            if p:
                bypkg[p] += b
            if u['g'] in msal_groups:
                c['msalTruth'] += b
                if p:
                    c['msalFound'] += b
                    if p in MSAL:
                        c['msalFoundAsMsal'] += b
                    else:
                        c[f'msalFoundAs:{p}'] += b
            elif p in MSAL:
                c['msalLabelOutsideTruth'] += b
            if t == '<app>':
                c['appBytes'] += b
                c['appFlagged'] += b if p else 0
        s = {
            'MSAL banner groups': len(msal_groups),
            'MSAL truth KB': round(c['msalTruth'] / 1e3, 1),
            'MSAL recall% (any library label)': round(100 * c['msalFound'] / c['msalTruth'], 1),
            'MSAL recall% (labelled msal)': round(100 * c['msalFoundAsMsal'] / c['msalTruth'], 1),
            'MSAL-labelled bytes outside MSAL truth': c['msalLabelOutsideTruth'],
            'MSAL truth labelled as other pkgs': {k.split(':', 1)[1]: v for k, v in c.items() if k.startswith('msalFoundAs:')},
            'app negatives flagged bytes': c['appFlagged'],
            'labelled KB: axios / zod / msal-common / msal-node': [round(bypkg[k] / 1e3, 1) for k in ('axios', 'zod', '@azure/msal-common', '@azure/msal-node')],
            'all library-labelled KB': round(sum(bypkg.values()) / 1e3, 1),
        }
        key = f'{iname} | {ver}'
        res[key] = s
        print(key, json.dumps(s), flush=True)
json.dump(res, open(ROOT + '/results/stepb-cc-exact.json', 'w'), indent=1)
