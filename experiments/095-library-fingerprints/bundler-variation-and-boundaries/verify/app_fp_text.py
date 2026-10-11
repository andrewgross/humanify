"""print the lab APP statements flagged by the recommended config with an esbuild-only index (Bun 1.4.0 combos)"""
import sys, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, index_from_builds, detect
from load import build_dirs
LAB = '/work/exp095-library-fingerprints/lab'
mins = [b for b in build_dirs() if '/min/' in b]
idx = index_from_builds([b for b in mins if b.startswith('esbuild-0.28.2/') and 'combo' not in b], 'h3')
for b in [b for b in mins if b.startswith('bun-1.4.0/') and '/combo' in b]:
    units = load_units(b); pred = detect(units, idx, strategy='S3', nmin=30, mincontent=3, minfncontent=0, support=10)
    txt = open(f'{LAB}/builds/{b}/out.js', encoding='utf8').read()
    for i, (u, p) in enumerate(zip(units, pred)):
        if p and u['truth'][0] == '<app>':
            print(b, p, u['e'] - u['s'], 'group', u['g'], repr(txt[u['s']:u['e']][:300]))
