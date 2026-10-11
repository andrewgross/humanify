"""VERSION DRIFT: the same library file at two versions of its package, same bundler config, same emission
kind. Is the source byte-identical? Is the hash equal? By version gap (patch / minor / major).
Output: results/drift.json + printed table."""
import json, collections, sys, itertools
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib, HERE

src = json.load(open(f'{HERE}/data/src-sha.json'))
CFGS = ['bun-1.4.0/min', 'esbuild-0.28.2/min']
VARS = ['raw', 'coreM', 'coreV', 'normM', 'normV']


def semver(v):
    core = v.split('-')[0].split('+')[0]
    p = (core.split('.') + ['0', '0'])[:3]
    try:
        return tuple(int(x) for x in p)
    except ValueError:
        return None


def gap(a, b):
    a, b = semver(a), semver(b)
    if a is None or b is None:
        return 'other'
    if a[0] != b[0]:
        return 'major'
    if a[1] != b[1]:
        return 'minor'
    if a[2] != b[2]:
        return 'patch'
    return 'prerelease'


def main():
    out = {}
    for cfg in CFGS:
        rows = [r for r in load() if r['ok'] and is_lib(r) and r['file'] and r['cfg'] == cfg and r['parts'] == 1]
        # modal hash per (pkg, file, kind, version)
        per = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
        for r in rows:
            for v in VARS:
                per[(r['pkg'], r['file'], r['kind'], r['ver'])][v][r[v]] += 1
        byfile = collections.defaultdict(dict)
        for (pkg, f, kind, ver), d in per.items():
            byfile[(pkg, f, kind)][ver] = {v: c.most_common(1)[0][0] for v, c in d.items()}
        stats = collections.defaultdict(lambda: collections.Counter())
        for (pkg, f, kind), vers in byfile.items():
            for a, b in itertools.combinations(sorted(vers), 2):
                g = gap(a, b)
                same_src = src.get(f'{pkg}@{a}/{f}') == src.get(f'{pkg}@{b}/{f}')
                s = stats[g]
                s['pairs'] += 1
                s['srcSame'] += same_src
                for v in VARS:
                    eq = vers[a][v] == vers[b][v]
                    s[f'{v}'] += eq
                    s[f'{v}|srcSame'] += eq and same_src
                    s[f'{v}|srcDiff'] += eq and not same_src
        # files per package version that are hash-identical to the PREVIOUS version present in the lab
        out[cfg] = {g: dict(s) for g, s in stats.items()}
        print(f'\n### {cfg}: same file, two versions, same emission kind ({sum(s["pairs"] for s in stats.values())} pairs)')
        print(f"{'gap':8s} {'pairs':>6s} {'srcSame':>8s} " + ' '.join(f'{v:>7s}' for v in VARS) + '   normM equal when source DIFFERS')
        for g in ['patch', 'minor', 'major', 'other']:
            s = stats.get(g)
            if not s:
                continue
            n = s['pairs']
            nd = n - s['srcSame']
            print(f"{g:8s} {n:6d} {100*s['srcSame']/n:7.1f}% " + ' '.join(f"{100*s[v]/n:6.1f}%" for v in VARS)
                  + f"   {s['normM|srcDiff']}/{nd} ({100*s['normM|srcDiff']/max(nd,1):.1f}%)"
                  + f"   [normM equal when source same: {s['normM|srcSame']}/{s['srcSame']}]")
    json.dump(out, open(f'{HERE}/results/drift.json', 'w'), indent=1)


if __name__ == '__main__':
    main()
