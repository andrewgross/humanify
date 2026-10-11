"""Sliding-window baseline for Step B (no statement structure): winnowed lexical k-gram hashes (scripts/window.mjs).

Index: every selected k-gram of library code in the index builds -> set(pkg); k-grams seen in non-library code veto.
Query: each selected k-gram found in the index (<= 3 packages, no veto) marks its character span with its packages.
A leaf statement is labelled P when >= 50% of its characters are covered by P's k-grams (W1); W3 adds the same
growth as the fingerprint detector (gap fill + wrapper groups). Scored per byte exactly like scripts/stepb.py.
usage: python3 scripts/window_stepb.py -> results/stepb-window.json
"""
import json, sys, collections, os
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, grow, score, summarize, NONLIB, ROOT
from load import build_dirs
from multiprocessing import Pool

LAB = '/work/exp095-library-fingerprints/lab'
WIN = ROOT + '/data/windows'


def labels_of(b):
    rows = [json.loads(l) for l in open(f'{LAB}/builds/{b}/labels.jsonl')]
    rows.sort(key=lambda r: r['startChar'])
    return rows


def windows_of(b):
    out = []
    for line in open(f'{WIN}/{b}.tsv'):
        p = line.rstrip('\n').split('\t')
        if len(p) == 3:
            out.append((int(p[0]), int(p[1]), p[2]))
    return out


def _index_one(b):
    import bisect
    labs = labels_of(b)
    starts = [r['startChar'] for r in labs]
    part = collections.defaultdict(set)
    for s, e, h in windows_of(b):
        i = bisect.bisect_right(starts, s) - 1
        pkg = labs[i]['pkg'] if i >= 0 and s < labs[i]['endChar'] else '<none>'
        part[h].add('<nonlib>' if pkg in NONLIB or pkg == '<none>' else pkg)
    return dict(part)


def build_index(builds):
    idx = collections.defaultdict(set)
    with Pool(48) as p:
        for part in p.imap_unordered(_index_one, builds, chunksize=8):
            for h, ps in part.items():
                idx[h] |= ps
    return idx


def detect_w(b, units, idx, stage, exclude=None, cover=0.5):
    cov = collections.defaultdict(lambda: collections.defaultdict(list))  # unit -> pkg -> [(s,e)]
    import bisect
    starts = [u['s'] for u in units]
    for s, e, h in windows_of(b):
        ps = idx.get(h)
        if not ps or '<nonlib>' in ps:
            continue
        if exclude:
            ps = ps - {exclude}
        if not ps or len(ps) > 3:
            continue
        i = bisect.bisect_right(starts, s) - 1
        # a k-gram may span several statements: credit each overlapped one
        while 0 <= i < len(units) and units[i]['s'] < e:
            u = units[i]
            a, z = max(s, u['s']), min(e, u['e'])
            if z > a:
                for p in ps:
                    cov[i][p].append((a, z))
            i += 1
    pred = [None] * len(units)
    prev = None
    for i, u in enumerate(units):
        iv = cov.get(i)
        if not iv:
            continue
        c = {}
        for p, xs in iv.items():
            xs.sort()
            tot, cs, ce = 0, None, None
            for a, z in xs:
                if cs is None or a > ce:
                    if cs is not None:
                        tot += ce - cs
                    cs, ce = a, z
                else:
                    ce = max(ce, z)
            tot += ce - cs
            c[p] = tot
        best = max(c.values())
        cands = sorted(k for k, v in c.items() if v == best)
        p = prev if prev in cands else cands[0]
        if best / max(1, u['e'] - u['s']) >= cover:
            pred[i] = p
            prev = p
    veto = [False] * len(units)
    return grow(units, pred, veto, stage)


def main():
    mins = [b for b in build_dirs() if '/min/' in b and not b.startswith('cc/')]
    singles = lambda cfgs: [b for b in mins if b.split('/')[0] in cfgs and not b.split('/')[2].startswith('combo')]
    combos = [b for b in mins if b.startswith('bun-1.4.0/') and b.split('/')[2].startswith('combo')]
    res = {}
    for iname, cfgs in (('other-bundlers (bun-1.3.14+bun-1.4.3+esbuild)', {'bun-1.3.14', 'bun-1.4.3', 'esbuild-0.28.2'}), ('same bundler (bun-1.4.0)', {'bun-1.4.0'}), ('esbuild only', {'esbuild-0.28.2'})):
        idx = build_index(singles(cfgs))
        print(f'### window index {iname}: {len(idx)} k-gram hashes', flush=True)
        for stage in (1, 3):
            tot = collections.Counter()
            loo = collections.Counter()
            for b in combos:
                units = load_units(b)
                tot += score(units, detect_w(b, units, idx, stage))
                for P in {u['truth'][0] for u in units if u['truth'][0] not in NONLIB and u['truth'][0] != '<none>'}:
                    for u, p in zip(units, detect_w(b, units, idx, stage, exclude=P)):
                        if u['truth'][0] == P:
                            loo['bytes'] += u['e'] - u['s']
                            if p:
                                loo['flagged'] += u['e'] - u['s']
            s = summarize(tot)
            s['LOO unknown-code flagged%'] = round(100 * loo['flagged'] / loo['bytes'], 2)
            s['index entries'] = len(idx)
            key = f'window k=20 w=8 | {iname} | bun-1.4.0 combos | W{stage}'
            res[key] = s
            print(key, json.dumps(s), flush=True)
    json.dump(res, open(ROOT + '/results/stepb-window.json', 'w'), indent=1)


if __name__ == '__main__':
    main()
