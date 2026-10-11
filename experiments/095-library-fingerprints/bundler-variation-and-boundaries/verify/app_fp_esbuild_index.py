"""VERIFY: which lab app bytes does the RECOMMENDED config flag when the index is esbuild-only (query: Bun 1.4.0 combos)?
Compares the recommended config (fn-content>=0) with the stepb_run config (content>=3 on functions too).
usage: PYTHONHASHSEED=0 python3 verify/app_fp_esbuild_index.py"""
import json, sys, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, index_from_builds, detect, score, summarize
from load import build_dirs
LAB = '/work/exp095-library-fingerprints/lab'
mins = [b for b in build_dirs() if '/min/' in b and not b.startswith('cc/')]
idx = index_from_builds([b for b in mins if b.startswith('esbuild-0.28.2/') and 'combo' not in b], 'h3')
out = {}
for name, kw in (('recommended (fn-content>=0, support=10)', dict(strategy='S3', nmin=30, mincontent=3, minfncontent=0, support=10)),
                 ('content>=3 on fns too, support=10', dict(strategy='S3', nmin=30, mincontent=3, support=10))):
    tot = collections.Counter(); ex = []
    for b in [b for b in mins if b.startswith('bun-1.4.0/') and '/combo' in b]:
        units = load_units(b); pred = detect(units, idx, **kw); tot += score(units, pred)
        txt = open(f'{LAB}/builds/{b.split("/",1)[0]}/{b.split("/",1)[1]}/out.js', encoding='utf8').read() if False else None
        for u, p in zip(units, pred):
            if p and u['truth'][0] in ('<app>', '<runtime>'):
                ex.append((b, u['truth'][0], p, u['e'] - u['s']))
    s = summarize(tot); s['flaggedNonlib'] = ex[:40]
    out[name] = s; print(name, json.dumps({k: v for k, v in s.items() if k != 'flaggedNonlib'}), flush=True)
    for e in ex[:15]: print('  ', e)
json.dump(out, open('/work/exp095-library-fingerprints/bundler-variation-and-boundaries/verify/app-fp-esbuild-index.json', 'w'), indent=1)
