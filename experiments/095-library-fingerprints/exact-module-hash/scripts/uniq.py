"""UNIQUENESS + INDEX SIZE. For a group of bundler configs and a hash variant:
  - index = distinct hashes of library spans
  - each hash classified: one file (possibly several versions) / duplicate source (several package files with
    byte-identical source) / TRUE collision (different source code, different files, same hash)
  - collisions by span size (the size floor)
  - lab app code (<app> spans) that hits the library index
Output: results/uniq.json, printed tables."""
import json, collections, sys
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib, HERE

src = json.load(open(f'{HERE}/data/src-sha.json'))
rows = [r for r in load() if r['ok'] and (r['purity'] is None or r['purity'] >= 0.9)]
BUCKETS = [0, 50, 100, 200, 500, 1000, 2000, 5000, 10 ** 9]
LBUCKETS = [0, 3, 100, 200, 400, 800, 1600, 3200, 6400, 10 ** 9]
SIZEKEY = {'raw': 'bytes', 'coreM': 'coreLen', 'coreV': 'coreLen', 'normM': 'normLen', 'normV': 'normLen', 'normStmt': 'normLen'}


def bucket(b, B=BUCKETS):
    for lo, hi in zip(B, B[1:]):
        if lo <= b < hi:
            return f'{lo}-{hi if hi < 10**9 else "inf"}'


GROUPS = {
    'bun-1.4.0/min': lambda c: c == 'bun-1.4.0/min',
    'bun-min-all (1.3.14,1.4.0,1.4.3)': lambda c: c.startswith('bun') and c.endswith('/min'),
    'esbuild/min': lambda c: c == 'esbuild-0.28.2/min',
    'bun+esbuild min': lambda c: (c.startswith('bun') or c.startswith('esbuild')) and c.endswith('/min'),
    'all 10 configs': lambda c: True,
}
VARS = ['raw', 'coreM', 'coreV', 'normM', 'normV', 'normStmt']
out = {}
for gname, pred in GROUPS.items():
    g = [r for r in rows if pred(r['cfg'])]
    lib = [r for r in g if is_lib(r) and r['file']]
    app = [r for r in g if r['pkg'] == '<app>']
    out[gname] = {}
    print(f'\n### {gname}: {len(lib)} library spans, {len(set((r["pkg"], r["ver"], r["file"]) for r in lib))} distinct pkg@ver/file')
    print(f"{'variant':9s} {'index':>8s} {'1 file':>8s} {'dupSrc':>7s} {'samePkg':>7s} {'xPkgCol':>7s} {'spans in collisions':>17s} {'app hits':>9s}")
    for v in VARS:
        by = collections.defaultdict(lambda: {'ids': set(), 'srcs': set(), 'minb': 10 ** 9, 'minl': 10 ** 9, 'n': 0})
        for r in lib:
            d = by[r[v]]
            d['ids'].add((r['pkg'], r['file']))
            d['srcs'].add(src.get(f"{r['pkg']}@{r['ver']}/{r['file']}"))
            d['minb'] = min(d['minb'], r['bytes'])
            d['minl'] = min(d['minl'], r[SIZEKEY[v]])
            d['n'] += 1
        cls = collections.Counter()
        spans_true = 0
        size_tab = collections.defaultdict(lambda: collections.Counter())
        len_tab = collections.defaultdict(lambda: collections.Counter())
        examples = []
        examples_same = []
        for h, d in by.items():
            if len(d['ids']) == 1:
                c = 'one-file'
            elif len(d['srcs']) == 1:
                c = 'dup-source'
            elif len(set(p for p, f in d['ids'])) == 1:
                c = 'same-pkg-collision'
                spans_true += d['n']
                if len(examples_same) < 400:
                    examples_same.append({'hash': h, 'minBytes': d['minb'], 'files': sorted(f'{p}/{f}' for p, f in d['ids'])[:6], 'nFiles': len(d['ids'])})
            else:
                c = 'true-collision'
                spans_true += d['n']
                if len(examples) < 400:
                    examples.append({'hash': h, 'minBytes': d['minb'], 'files': sorted(f'{p}/{f}' for p, f in d['ids'])[:6], 'nFiles': len(d['ids'])})
            cls[c] += 1
            size_tab[bucket(d['minb'])][c] += 1
            len_tab[bucket(d['minl'], LBUCKETS)][c] += 1
        idx = set(by)
        app_hits = [r for r in app if r[v] in idx]
        res = {'index': len(by), **cls, 'spansInTrueCollision': spans_true, 'libSpans': len(lib),
               'appSpans': len(app), 'appHits': len(app_hits),
               'bySize': {k: dict(c) for k, c in sorted(size_tab.items(), key=lambda x: int(x[0].split('-')[0]))},
               'byCanonLen': {k: dict(c) for k, c in sorted(len_tab.items(), key=lambda x: int(x[0].split('-')[0]))},
               'examples': sorted(examples, key=lambda e: -e['minBytes'])[:40], 'examplesSamePkg': sorted(examples_same, key=lambda e: -e['minBytes'])[:40]}
        out[gname][v] = res
        print(f"{v:9s} {len(by):8d} {cls['one-file']:8d} {cls['dup-source']:7d} {cls['same-pkg-collision']:7d} {cls['true-collision']:7d} "
              f"{spans_true:8d} ({100*spans_true/len(lib):4.1f}%) {len(app_hits):5d}/{len(app)}")
    for v, tab in [('coreM', 'bySize'), ('normM', 'bySize'), ('normM', 'byCanonLen')]:
        print(f'  {v} by {tab} (min over the hash class): bucket  hashes')
        for k, c in out[gname][v][tab].items():
            n = sum(c.values())
            print(f"    {k:>10s} {n:7d}  cross-pkg {100*c.get('true-collision',0)/n:6.2f}%  same-pkg {100*c.get('same-pkg-collision',0)/n:6.2f}%  dup-src {100*c.get('dup-source',0)/n:6.2f}%")
json.dump(out, open(f'{HERE}/results/uniq.json', 'w'), indent=1)
