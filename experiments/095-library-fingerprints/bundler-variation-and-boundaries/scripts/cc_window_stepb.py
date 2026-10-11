"""Sliding-window baseline on Claude Code: index = all lab builds' windows; precision bar = the 100 app negatives.
usage: PYTHONHASHSEED=0 python3 scripts/cc_window_stepb.py"""
import json, sys, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, NONLIB, ROOT
from window_stepb import build_index, detect_w
from load import build_dirs
mins = [b for b in build_dirs() if '/min/' in b and not b.startswith('cc/')]
idx = build_index(mins)
units = load_units('cc/min/cc-2.1.216/raw')
out = {}
for stage in (1, 3):
    pred = detect_w('cc/min/cc-2.1.216/raw', units, idx, stage)
    app = sum(u['e'] - u['s'] for u in units if u['truth'][0] == '<app>')
    fl = sum(u['e'] - u['s'] for u, p in zip(units, pred) if p and u['truth'][0] == '<app>')
    tot = sum(u['e'] - u['s'] for u, p in zip(units, pred) if p)
    out[f'W{stage}'] = {'app negatives flagged bytes': fl, 'app KB': round(app / 1e3, 1), 'flagged %': round(100 * fl / app, 2), 'all labelled KB': round(tot / 1e3, 1)}
    print(f'W{stage}', out[f'W{stage}'], flush=True)
json.dump(out, open(ROOT + '/results/stepb-cc-window.json', 'w'), indent=1)
