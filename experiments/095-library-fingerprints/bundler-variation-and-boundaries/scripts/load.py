"""Shared loader: per-build, per-module summaries from data/units (scan.mjs output).

A module key is (app, mode, pkg, pkgVersion, file). All parts of an eager file are merged.
Summary per module:
  kind, purity(min), bytes(char span total)
  mod[k]  for k in h0,h1,h2,h3,sk  : hash of the ordered list of depth-0 unit hashes
  fns     : list of (n, h1, h2, h3)  every function (>=12 L3 tokens) inside the module
  leaves  : list of (n, h1, h2, h3)  every leaf statement unit (non-wrapper), depth 0 or 1
"""
import json, os, hashlib, glob
from multiprocessing import Pool

ROOT = '/work/exp095-library-fingerprints/bundler-variation-and-boundaries'
UNITS = ROOT + '/data/units'


def h(s):
    return hashlib.md5(s.encode()).hexdigest()[:16]


def build_dirs():
    out = []
    for p in glob.glob(UNITS + '/*/*/*/*.jsonl'):
        if '/cc/' in p:
            continue
        out.append(os.path.relpath(p, UNITS)[:-6])  # bundler/min/app/mode
    return sorted(out)


def load_build(b):
    labs = json.load(open(f'{UNITS}/{b}.labels.json'))
    mods = {}
    bundler, mn, app, mode = b.split('/')
    rows = [json.loads(l) for l in open(f'{UNITS}/{b}.jsonl')]
    for r in rows:
        lab = labs.get(str(r['lab']))
        if lab is None:
            continue
        pkg, ver, file, kind, marker, purity, sc, ec = lab
        key = (app, mode, pkg, ver, file)
        m = mods.get(key)
        if m is None:
            m = mods[key] = {'kind': kind, 'purity': 1.0 if purity is None else purity, 'top': [], 'fns': [], 'leaves': [], 'bytes': 0, 'labs': set()}
        if purity is not None:
            m['purity'] = min(m['purity'], purity)
        if r['lab'] not in m['labs']:
            m['labs'].add(r['lab'])
            m['bytes'] += ec - sc
        if r['d'] == 0:
            m['top'].append((r['h0'], r['h1'], r['h2'], r['h3'], r['h4'], r['sk']))
        if not r['w']:
            m['leaves'].append((r['n'], r['h1'], r['h2'], r['h3'], r['h4']))
        if not r['w']:  # a wrapper's own function (__commonJS factory / __esm init) is bundler glue, not library code
            for f in r['fns']:
                m['fns'].append((f[2], f[3], f[4], f[5], f[6]))
    for m in mods.values():
        top = m.pop('top')
        m['mod'] = {k: h('|'.join(t[i] for t in top)) for i, k in enumerate(['h0', 'h1', 'h2', 'h3', 'h4', 'sk'])}
        m.pop('labs')
    return b, mods


def load_all(filter_fn=lambda b: True, procs=48):
    bs = [b for b in build_dirs() if filter_fn(b)]
    with Pool(procs) as p:
        return dict(p.map(load_build, bs, chunksize=4))
