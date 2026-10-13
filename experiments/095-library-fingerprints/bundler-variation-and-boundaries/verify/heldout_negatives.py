"""VERIFY: precision on Claude Code app code NOT used to choose the content rule.
Held-out sets (from layout-<v>.json lazy-ESM groups, by char span):
  H2  >=2 product-string kinds, no /*! banner, NOT one of the 100 picked negatives, ANY size, no vendor-literal filter
  H1  >=1 product-string kind, no /*! banner, not picked
Index = whole lab (as stepb_final.py does for Claude Code). Config = the recommended one.
usage: PYTHONHASHSEED=0 python3 verify/heldout_negatives.py <ver>"""
import json, sys, re, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, index_from_builds, detect, UNITS
from load import build_dirs
ver = sys.argv[1]
LAB = '/work/exp095-library-fingerprints/lab'
BUNDLE = f'/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-{ver}/binary-decompiled/src/entrypoints/index.js'
src = open(BUNDLE, encoding='utf8').read()
PRODUCT = [r'tengu_[a-z0-9_]+', r'Claude Code', r'claude\.ai', r'CLAUDE\.md', r'ANTHROPIC_[A-Z_]+', r'\.claude/', r'claude-code', r'Anthropic']
lay = json.load(open(f'{LAB}/realworld/layout-{ver}.json'))
picked = set()
for l in open(f'{LAB}/realworld/cc-{ver}/negatives.jsonl'):
    r = json.loads(l); picked.add(open(f"{LAB}/{r['textPath']}").read())
sets = {'H2': [], 'H1': []}
for m in lay['modules']:
    if m['kind'] != 'lazy-esm':
        continue
    t = src[m['startChar']:m['endChar']]
    if '/*!' in t or '@license' in t or t in picked:
        continue
    k = sum(1 for p in PRODUCT if re.search(p, t))
    if k >= 2: sets['H2'].append((m['startChar'], m['endChar']))
    if k >= 1: sets['H1'].append((m['startChar'], m['endChar']))
mins = [b for b in build_dirs() if '/min/' in b and not b.startswith('cc/')]
idx = index_from_builds(mins, 'h3')
units = load_units(f'cc/min/cc-{ver}/raw')
CONFIGS = {'recommended': dict(strategy='S3', nmin=30, mincontent=3, minfncontent=0, support=10),
           'no-content-rule': dict(strategy='S3', nmin=30, mincontent=0)}
import bisect
res = {}
for cname, kw in CONFIGS.items():
    pred = detect(units, idx, **kw)
    for sname, spans in sets.items():
        spans.sort(); starts = [s for s, e in spans]
        tot = fl = 0; ex = collections.Counter(); exs = []
        for u, p in zip(units, pred):
            k = bisect.bisect_right(starts, u['s']) - 1
            if k < 0 or not (spans[k][0] <= u['s'] < spans[k][1]):
                continue
            b = u['e'] - u['s']; tot += b
            if p:
                fl += b; ex[p] += b
                if len(exs) < 25: exs.append((p, b, src[u['s']:u['e']][:260]))
        res[f'{cname} | {sname}'] = {'groups': len(spans), 'KB': round(tot / 1e3, 1), 'flaggedBytes': fl, 'flagged%': round(100 * fl / max(tot, 1), 3), 'byPkg': ex.most_common(12), 'examples': exs}
        print(cname, sname, len(spans), round(tot / 1e3, 1), 'KB flagged', fl, ex.most_common(8), flush=True)
json.dump(res, open(f'/work/exp095-library-fingerprints/bundler-variation-and-boundaries/verify/heldout-negatives-{ver}.json', 'w'), indent=1)
