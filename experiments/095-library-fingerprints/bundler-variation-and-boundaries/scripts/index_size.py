"""How big would a shipped index be? Counts distinct fingerprints the Step B detector uses (functions and leaf statements
with >= NMIN canonical tokens, level h3) per package version, per package (all versions), and how much each extra
bundler adds. usage: python3 scripts/index_size.py -> results/index-size.json"""
import json, sys, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, NONLIB, ROOT
from load import build_dirs
from multiprocessing import Pool

NMIN = 30
BUNDLERS = ['bun-1.4.0', 'bun-1.3.14', 'bun-1.4.3', 'esbuild-0.28.2', 'rollup-4.64.5']


def one(b):
    bundler = b.split('/')[0]
    out = []
    for u in load_units(b):
        pkg, ver, _ = u['truth']
        if pkg in NONLIB or pkg == '<none>':
            continue
        lvl = 'h3'  # one hash space for every bundler, so union growth is comparable
        fi = 6 if lvl == 'h4' else 5
        for f in u['fns']:
            if f[2] >= NMIN:
                out.append(('f', f[fi], pkg, ver, bundler, f[2]))
        if u['n'] >= NMIN:
            out.append(('s', u[lvl], pkg, ver, bundler, u['n']))
    return b, out


def main():
    builds = [b for b in build_dirs() if '/min/' in b and not b.startswith('cc/')]
    pv = collections.defaultdict(set)  # pkg@ver -> hashes (all bundlers)
    pv_b = collections.defaultdict(lambda: collections.defaultdict(set))  # bundler -> pkg@ver -> hashes
    hpk = collections.defaultdict(set)  # hash -> pkgs
    hsize = {}
    with Pool(48) as p:
        for b, rows in p.imap_unordered(one, builds, chunksize=8):
            for kind, h, pkg, ver, bundler, n in rows:
                key = (kind, h)
                pv[(pkg, ver)].add(key)
                pv_b[bundler][(pkg, ver)].add(key)
                hpk[key].add(pkg)
                hsize[key] = n
    res = {}
    # 1. per package version (all bundlers) and per bundler
    counts = sorted(len(v) for v in pv.values())
    q = lambda xs, f: xs[min(len(xs) - 1, int(f * len(xs)))]
    res['pkgVersions'] = len(pv)
    res['hashes per pkg@ver (all 5 bundlers)'] = {'median': q(counts, .5), 'p90': q(counts, .9), 'max': counts[-1], 'total': sum(counts)}
    for bd in BUNDLERS:
        c = sorted(len(v) for v in pv_b[bd].values())
        res[f'hashes per pkg@ver ({bd} only)'] = {'median': q(c, .5), 'p90': q(c, .9), 'max': c[-1], 'total distinct': len(set().union(*pv_b[bd].values()))}
    # 2. union growth as bundlers are added (distinct hashes over all packages)
    acc = set()
    growth = []
    for bd in BUNDLERS:
        new = set().union(*pv_b[bd].values())
        before = len(acc)
        acc |= new
        growth.append((bd, len(acc), len(acc) - before))
    res['distinct hashes as bundlers are added'] = growth
    # 3. sharing between versions of a package
    bypkg = collections.defaultdict(dict)
    for (pkg, ver), hs in pv.items():
        bypkg[pkg][ver] = hs
    share = []
    for pkg, vs in bypkg.items():
        if len(vs) < 2:
            continue
        union = set().union(*vs.values())
        tot = sum(len(h) for h in vs.values())
        share.append((pkg, len(vs), tot, len(union)))
    res['versions dedup'] = {
        'packages with >=2 versions': len(share),
        'sum of per-version counts': sum(s[2] for s in share),
        'distinct after dedup': sum(s[3] for s in share),
        'dedup ratio': round(sum(s[3] for s in share) / max(1, sum(s[2] for s in share)), 3),
    }
    # 4. ambiguity: share of hashes found in >1 package, by size
    amb = collections.Counter()
    tot = collections.Counter()
    for k, ps in hpk.items():
        n = hsize[k]
        bin_ = '30-49' if n < 50 else '50-99' if n < 100 else '100-299' if n < 300 else '300+'
        tot[bin_] += 1
        if len(ps) > 1:
            amb[bin_] += 1
    res['hashes in >1 package, by size (tokens)'] = {b: f'{amb[b]}/{tot[b]} ({100*amb[b]/tot[b]:.1f}%)' for b in sorted(tot)}
    res['distinct hashes total'] = len(hpk)
    res['bytes per entry (8-byte hash + 4-byte package-version id)'] = 12
    res['index MB (all 5 bundlers, lab catalog)'] = round(len(hpk) * 12 / 1e6, 2)
    # per-package biggest
    res['largest packages (distinct hashes, all versions+bundlers)'] = sorted(((pkg, len(set().union(*vs.values()))) for pkg, vs in bypkg.items()), key=lambda x: -x[1])[:12]
    json.dump(res, open(ROOT + '/results/index-size.json', 'w'), indent=1)
    print(json.dumps(res, indent=1))


main()
