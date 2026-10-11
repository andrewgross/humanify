"""INDEX SIZE: how many hashes the lab's packages need, how that grows per added version, and what an
index entry costs. Output: results/indexsize.json + printed tables."""
import json, collections, sys, gzip, struct, os
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib, HERE
from drift import semver, gap

rows = [r for r in load() if r['ok'] and is_lib(r) and r['file'] and (r['purity'] is None or r['purity'] >= 0.9)]
FLOOR = 400  # minimum canonical length (normLen) of an entry (uniq.py: cross-package collisions stop at ~400)
GROUPS = {
    'bun-1.4.0/min': lambda c: c == 'bun-1.4.0/min',
    'bun all minified': lambda c: c.startswith('bun') and c.endswith('/min'),
    'bun+esbuild minified': lambda c: (c.startswith('bun') or c.startswith('esbuild')) and c.endswith('/min'),
    'all 10 configs': lambda c: True,
}
out = {'floorBytes': FLOOR, 'groups': {}, 'growth': {}}
print(f"{'group':24s} {'pkg@ver':>8s} {'files':>7s} {'normM':>7s} {'>=floor':>8s} {'per pkg@ver':>11s} {'coreM':>7s} {'raw':>7s}")
for g, pred in GROUPS.items():
    gr = [r for r in rows if pred(r['cfg'])]
    pv = {(r['pkg'], r['ver']) for r in gr}
    files = {(r['pkg'], r['ver'], r['file']) for r in gr}
    nm = {r['normM'] for r in gr}
    nm_floor = {r['normM'] for r in gr if r['normLen'] >= FLOOR}
    cm = {r['coreM'] for r in gr}
    raw = {r['raw'] for r in gr}
    out['groups'][g] = {'pkgVersions': len(pv), 'files': len(files), 'normM': len(nm), 'normMFloor': len(nm_floor),
                        'coreM': len(cm), 'raw': len(raw)}
    print(f"{g:24s} {len(pv):8d} {len(files):7d} {len(nm):7d} {len(nm_floor):8d} {len(nm)/len(pv):11.1f} {len(cm):7d} {len(raw):7d}")

# growth per added version, Bun minified family, per package (direct + transitive), versions in semver order
gr = [r for r in rows if r['cfg'].startswith('bun') and r['cfg'].endswith('/min')]
per = collections.defaultdict(lambda: collections.defaultdict(set))
for r in gr:
    per[r['pkg']][r['ver']].add(r['normM'])
added = collections.defaultdict(lambda: [0, 0])  # gap -> [new hashes, hashes in that version]
first = []
for pkg, vers in per.items():
    order = sorted(vers, key=lambda v: semver(v) or (0, 0, 0))
    seen = set()
    prev = None
    for v in order:
        hs = vers[v]
        if prev is None:
            first.append(len(hs))
        else:
            g = gap(prev, v)
            added[g][0] += len(hs - seen)
            added[g][1] += len(hs)
        seen |= hs
        prev = v
out['growth'] = {g: {'newHashes': a, 'hashesInVersion': b, 'newFraction': a / b} for g, (a, b) in added.items()}
print('\nGrowth when one more version of a package is added (Bun minified family, normM):')
for g, d in out['growth'].items():
    print(f"  {g:10s} {d['newHashes']:6d} new of {d['hashesInVersion']:6d} = {100*d['newFraction']:.1f}% new")
print(f"  first version of a package: median {sorted(first)[len(first)//2]} hashes, mean {sum(first)/len(first):.1f}")

# per-entry cost: a real packed index for 'bun all minified' (8-byte hash + 4-byte file-id), gzip
gr = [r for r in rows if GROUPS['bun all minified'](r['cfg']) and r['normLen'] >= FLOOR]
fid = {}
entries = {}
for r in gr:
    k = (r['pkg'], r['file'])
    fid.setdefault(k, len(fid))
    entries.setdefault(r['normM'], set()).add((fid[k], r['ver']))
blob = b''.join(struct.pack('<QI', int(h, 16), min(f for f, _ in v)) for h, v in sorted(entries.items()))
names = '\n'.join(f'{p}\t{f}' for (p, f) in fid).encode()
vers = json.dumps({h: sorted({v for _, v in s}) for h, s in entries.items()}).encode()
out['packed'] = {'entries': len(entries), 'hashBlobBytes': len(blob), 'hashBlobGz': len(gzip.compress(blob)),
                 'namesGz': len(gzip.compress(names)), 'versionListsGz': len(gzip.compress(vers)),
                 'bytesPerEntryAll': (len(gzip.compress(blob)) + len(gzip.compress(names)) + len(gzip.compress(vers))) / len(entries)}
print('\nPacked index (bun all minified, >= floor):', out['packed'])
json.dump(out, open(f'{HERE}/results/indexsize.json', 'w'), indent=1)
