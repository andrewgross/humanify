"""Inspect Step B false positives: app code flagged, and leave-one-out confusions, with code snippets."""
import json, sys, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, index_from_builds, detect, NONLIB
from load import build_dirs
LAB = '/work/exp095-library-fingerprints/lab'
mins = [b for b in build_dirs() if '/min/' in b]
idxcfg = sys.argv[1].split(',')
qcfg = sys.argv[2]
strategy = sys.argv[3] if len(sys.argv) > 3 else 'S3'
idx = index_from_builds([b for b in mins if b.split('/')[0] in idxcfg and not b.split('/')[2].startswith('combo')], 'h3')
conf = collections.Counter(); ex = {}
for b in [b for b in mins if b.startswith(qcfg + '/') and b.split('/')[2].startswith('combo')]:
    units = load_units(b)
    src = open(f'{LAB}/builds/{b}/out.js', encoding='utf8').read()
    pred = detect(units, idx, nmin=30, strategy=strategy)
    for u, p in zip(units, pred):
        if p and u['truth'][0] in NONLIB:
            print('APP-FP', b, u['truth'][0], '->', p, repr(src[u['s']:u['e']][:300]))
    pkgs = {u['truth'][0] for u in units if u['truth'][0] not in NONLIB and u['truth'][0] != '<none>'}
    for P in pkgs:
        pr = detect(units, idx, nmin=30, strategy=strategy, exclude=P)
        for u, p in zip(units, pr):
            if u['truth'][0] == P and p:
                k = (P, u['truth'][1], p)
                conf[k] += u['e'] - u['s']
                if k not in ex or len(ex[k]) < 2:
                    ex.setdefault(k, []).append(src[u['s']:u['e']][:220])
for k, v in conf.most_common(25):
    print(f'\n{v:8d}  {k[0]}@{k[1]}  flagged as  {k[2]}')
    for e in ex[k]:
        print('          ', repr(e))
