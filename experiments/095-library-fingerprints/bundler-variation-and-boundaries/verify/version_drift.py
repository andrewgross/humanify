"""VERIFY: the lab's Step B headline uses an index that holds the EXACT package versions of the combo queries
(combos are built from the 'latest' versions, which are also single-package apps). Real bundles ship versions the
index may not hold. Here the index (bun-1.3.14 + bun-1.4.3 + esbuild singles, as the headline) DROPS every unit whose
(pkg, version) occurs in any combo query, so each package can only be recognised from its OTHER versions
(earlier patch / minor / major). Also re-runs the headline index unchanged as a control.
usage: PYTHONHASHSEED=0 python3 verify/version_drift.py"""
import json, sys, collections
from multiprocessing import Pool
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
import stepb
from stepb import load_units, detect, score, summarize, NONLIB
from load import build_dirs
mins = [b for b in build_dirs() if '/min/' in b and not b.startswith('cc/')]
singles = lambda cfgs: [b for b in mins if b.split('/')[0] in cfgs and not b.split('/')[2].startswith('combo')]
combos = lambda cfg: [b for b in mins if b.startswith(cfg + '/') and b.split('/')[2].startswith('combo')]
qb = {b: load_units(b) for b in combos('bun-1.4.0')}
qver = set()
for us in qb.values():
    for u in us:
        if u['truth'][0] not in NONLIB and u['truth'][0] != '<none>':
            qver.add((u['truth'][0], u['truth'][1]))
DROP = set()

def one(b):
    fi = collections.defaultdict(set); si = collections.defaultdict(set); li = collections.defaultdict(set); fn_n = {}
    dropped = 0
    for u in load_units(b):
        pkg = u['truth'][0]
        if (pkg, u['truth'][1]) in DROP:
            dropped += 1
            continue
        tag = '<nonlib>' if pkg in NONLIB or pkg == '<none>' else pkg
        for f in u['fns']:
            fi[f[5]].add(tag); fn_n[f[5]] = f[2]
        si[u['h3']].add(tag)
        for s in u['ls']:
            li[s].add(tag)
    return dict(fi), dict(si), dict(li), fn_n, dropped

def build(builds):
    fi = collections.defaultdict(set); si = collections.defaultdict(set); li = collections.defaultdict(set); fn_n = {}; dr = 0
    with Pool(48) as p:
        for a, b2, c, d, n in p.imap_unordered(one, builds, chunksize=8):
            fn_n.update(d); dr += n
            for k, v in a.items(): fi[k] |= v
            for k, v in b2.items(): si[k] |= v
            for k, v in c.items(): li[k] |= v
    return {'fn': dict(fi), 'stmt': dict(si), 'str': dict(li), 'level': 'h3', 'fnN': fn_n}, dr
KW = dict(strategy='S3', nmin=30, mincontent=3, minfncontent=0, support=10)
res = {'query package versions': len(qver)}
for name, drop in (('control (exact versions in index)', set()), ('exact query versions DROPPED', qver)):
    DROP.clear(); DROP.update(drop)
    for iname, cf in (('bun-1.3.14+1.4.3+esbuild', {'bun-1.3.14', 'bun-1.4.3', 'esbuild-0.28.2'}), ('esbuild only', {'esbuild-0.28.2'})):
        idx, dr = build(singles(cf))
        tot = collections.Counter(); perpkg = collections.Counter(); perpkgF = collections.Counter()
        for b, units in qb.items():
            pred = detect(units, idx, **KW)
            tot += score(units, pred)
            for u, p in zip(units, pred):
                t = u['truth'][0]
                if t not in NONLIB and t != '<none>':
                    perpkg[t] += u['e'] - u['s']; perpkgF[t] += (u['e'] - u['s']) if p else 0
        s = summarize(tot); s['unitsDropped'] = dr
        s['worst packages (recall%, KB)'] = sorted(((k, round(100 * perpkgF[k] / v, 1), round(v / 1e3, 1)) for k, v in perpkg.items() if v > 20000), key=lambda x: x[1])[:12]
        key = f'{name} | index {iname}'
        res[key] = s
        print(key, json.dumps(s), flush=True)
json.dump(res, open('/work/exp095-library-fingerprints/bundler-variation-and-boundaries/verify/version-drift.json', 'w'), indent=1)
