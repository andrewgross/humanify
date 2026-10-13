"""INDEX SIZE EXTRAPOLATION to the npm top-N.
1. Calibrate on the lab: hashes per package version (Bun minified family, normM, canonical length >= 400)
   against the registry's dist.fileCount of that version -> ratio r (hashes per published file).
2. For the top-1000 npm packages by downloads (+500 random from ranks 1001-15916): published versions
   (no prereleases), their fileCount and the patch/minor/major steps between consecutive versions.
3. Entries for a package = r * fileCount(first version kept) + sum over later versions of
   r * fileCount * newFraction(step), newFraction measured in the lab (indexsize.py: patch/minor/major).
Output: results/extrapolate.json + printed table."""
import json, os, sys, collections, statistics
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib, HERE
from drift import semver

MINLEN = 400
growth = json.load(open(f'{HERE}/results/indexsize.json'))['growth']
NEW = {g: d['newFraction'] for g, d in growth.items()}


def meta(dirn, name):
    p = f'{HERE}/npmtop/{dirn}/{name.replace("/", "__")}.json'
    try:
        return json.load(open(p))
    except Exception:
        return None


# 1. calibration
rows = [r for r in load() if r['ok'] and is_lib(r) and r['file'] and r['cfg'].startswith('bun') and r['cfg'].endswith('/min')]
per = collections.defaultdict(set)
for r in rows:
    if r['normLen'] >= MINLEN:
        per[(r['pkg'], r['ver'])].add(r['normM'])
ratios, tot_h, tot_f = [], 0, 0
for (pkg, ver), hs in per.items():
    m = meta('metalab', pkg)
    fc = m and m.get('versions', {}).get(ver, {}).get('dist', {}).get('fileCount')
    if fc:
        ratios.append(len(hs) / fc)
        tot_h += len(hs)
        tot_f += fc
r_pooled = tot_h / tot_f
print(f'calibration: {len(ratios)} lab package versions; hashes per published file: pooled {r_pooled:.3f}, '
      f'median {statistics.median(ratios):.3f}')


def gap(a, b):
    if a[0] != b[0]:
        return 'major'
    if a[1] != b[1]:
        return 'minor'
    return 'patch'


def package_entries(m, last_n=None, r=r_pooled):
    vs = []
    for v, d in (m.get('versions') or {}).items():
        sv = semver(v)
        if sv is None or '-' in v:
            continue
        fc = (d.get('dist') or {}).get('fileCount')
        if fc:
            vs.append((sv, fc))
    vs.sort()
    if last_n:
        vs = vs[-last_n:]
    if not vs:
        return 0, 0
    e = r * vs[0][1]
    for (pa, _), (pb, fb) in zip(vs, vs[1:]):
        e += r * fb * NEW[gap(pa, pb)]
    return e, len(vs)


names = [l.strip() for l in open(f'{HERE}/npmtop/sample.txt') if l.strip()]
top, rand = names[:1000], names[1000:]
out = {'ratioPooled': r_pooled, 'ratioMedian': statistics.median(ratios), 'newFraction': NEW, 'rows': {}}
for label, group in [('top 1000', top), ('random 500 of ranks 1001-15916', rand)]:
    for last_n in [1, 5, 20, None]:
        es, nv, fcs = [], [], []
        for n in group:
            m = meta('meta', n)
            if not m:
                continue
            e, k = package_entries(m, last_n)
            es.append(e)
            nv.append(k)
        out['rows'][f'{label}|{last_n}'] = {'packages': len(es), 'entries': sum(es), 'medianPerPkg': statistics.median(es),
                                            'meanVersions': sum(nv) / len(nv)}
        print(f"{label:34s} versions={'all' if last_n is None else 'last '+str(last_n):8s} packages {len(es):4d} "
              f"entries {sum(es):10,.0f} (mean {sum(es)/len(es):7.1f}/pkg, median {statistics.median(es):6.1f}; "
              f"mean versions kept {sum(nv)/len(nv):.1f})")
# extrapolate to the full top-15,916 list: top-1000 measured + 14,916 x random-sample mean
for last_n in [1, 5, 20, None]:
    a = out['rows'][f'top 1000|{last_n}']['entries']
    b = out['rows'][f'random 500 of ranks 1001-15916|{last_n}']
    total = a + 14916 * b['entries'] / b['packages']
    out['rows'][f'top15916|{last_n}'] = {'entries': total}
    print(f"top 15,916 (npm-high-impact downloads list), versions={'all' if last_n is None else 'last '+str(last_n)}: "
          f"~{total/1e6:.2f} M entries, ~{total*16/1e6:.0f} MB at 16 B/entry raw")
json.dump(out, open(f'{HERE}/results/extrapolate.json', 'w'), indent=1)
