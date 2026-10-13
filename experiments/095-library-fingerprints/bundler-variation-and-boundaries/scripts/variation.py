"""Part 1: how the SAME module differs across bundlers / modes / library versions, per fingerprint level.

usage: python3 scripts/variation.py   -> results/variation.json + results/variation.md
"""
import json, collections, sys
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from load import load_all, ROOT

COUNT = False
LIB = lambda pkg: not pkg.startswith('<')
LEVELS = ['h0', 'h1', 'h2', 'h3', 'h4', 'sk']
FLEVELS = ['h1', 'h2', 'h3', 'h4']


def contain(a, b, li, min_n=0):
    """token-weighted share of multiset a found in multiset b at level index li"""
    cb = collections.Counter(x[li] for x in b)
    tot = hit = 0
    for x in a:
        if x[0] < min_n:
            continue
        w = 1 if COUNT else x[0]
        tot += w
        if cb[x[li]] > 0:
            cb[x[li]] -= 1
            hit += w
    return hit, tot


class Acc:
    def __init__(self):
        self.n = 0
        self.bytes = 0
        self.mod = collections.Counter()
        self.fn = collections.defaultdict(lambda: [0, 0])
        self.fn50 = collections.defaultdict(lambda: [0, 0])
        self.leaf = collections.defaultdict(lambda: [0, 0])
        self.fnc = collections.defaultdict(lambda: [0, 0])

    def add(self, A, B):
        self.n += 1
        self.bytes += A['bytes']
        for k in LEVELS:
            if A['mod'][k] == B['mod'][k]:
                self.mod[k] += 1
        for i, k in enumerate(FLEVELS, start=1):
            for acc, src_a, src_b, mn in ((self.fn, A['fns'], B['fns'], 0), (self.fn50, A['fns'], B['fns'], 50), (self.leaf, A['leaves'], B['leaves'], 0)):
                hh, tt = contain(src_a, src_b, i, mn)
                acc[k][0] += hh
                acc[k][1] += tt
            global COUNT
            COUNT = True
            hh, tt = contain(A['fns'], B['fns'], i, 0)
            COUNT = False
            self.fnc[k][0] += hh
            self.fnc[k][1] += tt

    def out(self):
        pct = lambda a: round(100 * a[0] / a[1], 1) if a[1] else None
        return {
            'modules': self.n,
            'MB': round(self.bytes / 1e6, 1),
            'moduleExact%': {k: round(100 * self.mod[k] / self.n, 1) if self.n else None for k in LEVELS},
            'fnContain%': {k: pct(self.fn[k]) for k in FLEVELS},
            'fnCountContain%': {k: pct(self.fnc[k]) for k in FLEVELS},
            'fn50Contain%': {k: pct(self.fn50[k]) for k in FLEVELS},
            'leafContain%': {k: pct(self.leaf[k]) for k in FLEVELS},
        }


def main():
    data = load_all(lambda b: '/min/' in b)
    by = collections.defaultdict(dict)  # bundler -> key -> mod
    for b, mods in data.items():
        bundler = b.split('/')[0]
        for key, m in mods.items():
            if LIB(key[2]) and m['kind'] in ('cjs', 'lazy-esm', 'eager-esm'):
                by[bundler][key] = m
    res = {}
    pairs = [
        ('bun-1.4.0', 'bun-1.4.3'),
        ('bun-1.3.14', 'bun-1.4.0'),
        ('bun-1.4.0', 'esbuild-0.28.2'),
        ('bun-1.4.0', 'rollup-4.64.5'),
        ('esbuild-0.28.2', 'rollup-4.64.5'),
    ]
    for a, b in pairs:
        accs = collections.defaultdict(Acc)
        for key, A in by[a].items():
            B = by[b].get(key)
            if B is None:
                continue
            if 'rollup' in a + b and min(A['purity'], B['purity']) < 0.9:
                continue
            accs['all'].add(A, B)
            accs[A['kind']].add(A, B)
            if A['kind'] != B['kind']:
                accs['kindDiffers'].add(A, B)
        res[f'{a} vs {b}'] = {k: v.out() for k, v in accs.items()}
        print(a, b, json.dumps(res[f'{a} vs {b}']['all']), flush=True)

    # cross-mode within one bundler (same app, same file, different entry mode)
    for bundler in ('bun-1.4.0', 'esbuild-0.28.2'):
        for ma, mb in (('dyn', 'req'), ('req', 'imp'), ('imp', 'some'), ('dyn', 'imp')):
            accs = collections.defaultdict(Acc)
            for key, A in by[bundler].items():
                if key[1] != ma:
                    continue
                B = by[bundler].get((key[0], mb) + key[2:])
                if B is None:
                    continue
                accs['all'].add(A, B)
                accs[f"{A['kind']}->{B['kind']}"].add(A, B)
            res[f'{bundler} mode {ma} vs {mb}'] = {k: v.out() for k, v in accs.items()}
            print(bundler, ma, mb, json.dumps(res[f'{bundler} mode {ma} vs {mb}']['all']), flush=True)

    # cross-version: same bundler, same mode, same pkg+file, different pkgVersion (single-package apps only)
    for bundler in ('bun-1.4.0',):
        files = collections.defaultdict(dict)  # (pkg,file,mode) -> ver -> mod
        for key, m in by[bundler].items():
            app, mode, pkg, ver, file = key
            if app.startswith('combo'):
                continue
            files[(pkg, file, mode)].setdefault(ver, m)
        vers = json.load(open('/work/exp095-library-fingerprints/lab/versions.json'))
        accs = collections.defaultdict(Acc)

        def vkey(v):
            return tuple(int(x) if x.isdigit() else 0 for x in v.split('-')[0].split('.'))

        for (pkg, file, mode), vm in files.items():
            vs = sorted(vm, key=vkey)
            for v1, v2 in zip(vs, vs[1:]):
                a1, a2 = vkey(v1), vkey(v2)
                step = 'major' if a1[0] != a2[0] else 'minor' if a1[:2] != a2[:2] else 'patch'
                accs[step].add(vm[v2], vm[v1])
                accs['all'].add(vm[v2], vm[v1])
        res[f'{bundler} adjacent library versions (newer vs older)'] = {k: v.out() for k, v in accs.items()}
        print('versions', json.dumps({k: v.out() for k, v in accs.items()}), flush=True)

    json.dump(res, open(ROOT + '/results/variation.json', 'w'), indent=1)


if __name__ == '__main__':
    main()
