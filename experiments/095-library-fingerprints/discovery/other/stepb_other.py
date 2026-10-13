"""exp095 discovery: the negatives lane's Step B, index C (stepb_cc.py's exact config and builds), on ANOTHER
Claude Code release's units. Writes {unit: package|null} like data/stepb-pred-C.json.
usage: PYTHONHASHSEED=0 python3 -I stepb_other.py <dir holding raw.jsonl> <out.json>"""
import glob
import json
import os
import sys

sys.path.insert(0, '/work/exp095-library-fingerprints/negatives/scripts')
from stepb import UNITS, detect, index_from_builds, load_units  # noqa: E402

CONFIG = dict(strategy='S3', nmin=30, mincontent=3, minfncontent=0, support=10)
rel = lambda p: os.path.relpath(p, UNITS)[:-6]  # noqa: E731
labb = sorted(rel(p) for p in glob.glob(UNITS + '/*/min/*/*.jsonl') if '/cc/' not in p and '/exact/' not in p and '/exactver/' not in p)
bvbex = sorted(rel(p) for p in glob.glob(UNITS + '/exactver/*/min/*/*.jsonl'))
mine = sorted(rel(p) for p in glob.glob(UNITS + '/exact/bun-1.4.0/min/*/*.jsonl'))
mineb = sorted(rel(p) for p in glob.glob(UNITS + '/exact/bun-1.4.0-tbun/min/*/*.jsonl'))
units_dir, out = sys.argv[1], sys.argv[2]
raw_rel = os.path.relpath(os.path.join(units_dir, 'raw'), UNITS)
idx = index_from_builds(labb + bvbex + mine + mineb, 'h3')
units = load_units(raw_rel)
raw = [json.loads(l) for l in open(os.path.join(units_dir, 'raw.jsonl'))]
leaf_i = [r['i'] for r in raw if not (r['d'] == 0 and r['w'])]
assert len(leaf_i) == len(units)
pred = detect(units, idx, **CONFIG)
json.dump({i: p for i, p in zip(leaf_i, pred)}, open(out, 'w'))
print('claimed', sum(1 for p in pred if p), 'of', len(pred))
