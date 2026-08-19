#!/usr/bin/env python3
"""
080 — DISPLACEMENT: a prior release's module loses its filename to a newcomer.

  python3 experiments/080-noise-sources/displaced-names.py <priorSrc> <freshSrc>

For each fresh file named `X-N.js` whose base `X.js` existed in the prior
release, ask which module the suffixed file actually holds — by comparing its
EXPORT SET to the prior `X.js`. If the suffixed file holds the prior module's
exports, the prior module was displaced and a newcomer took its name.

Written after declaring this mechanism dead. One sample (strip-ansi) showed the
rule working; the largest cross-file move in the tree
(pr-review-artifact-template.js -> -2.js, 916 lines) shows it failing. One
sample is not a population, in either direction.
"""
import os, re, sys, collections

SUFFIX = re.compile(r'^(.*)-(\d+)\.js$')
EXPORT = re.compile(r'module\.exports,\s*"([A-Za-z0-9_$]+)"')

def exports_of(path):
    try:
        with open(path, encoding="utf8", errors="ignore") as fh:
            return frozenset(EXPORT.findall(fh.read()))
    except OSError:
        return frozenset()

def walk(root):
    for dp, _, fns in os.walk(root):
        for fn in fns:
            if fn.endswith(".js"):
                yield os.path.relpath(os.path.join(dp, fn), root)

prior_root, fresh_root = sys.argv[1], sys.argv[2]
prior = set(walk(prior_root))
displaced = []
suffixed_total = 0

for rel in walk(fresh_root):
    m = SUFFIX.match(rel)
    if not m:
        continue
    suffixed_total += 1
    base = f"{m.group(1)}.js"
    if base not in prior:
        continue                      # nothing to displace
    pe = exports_of(os.path.join(prior_root, base))
    se = exports_of(os.path.join(fresh_root, rel))
    be = exports_of(os.path.join(fresh_root, base)) if base in set(walk(fresh_root)) else frozenset()
    if not pe or not se:
        continue
    j_suffixed = len(pe & se) / max(1, len(pe | se))
    j_base = len(pe & be) / max(1, len(pe | be)) if be else 0.0
    # The suffixed file holds the prior module more than the base file does.
    if j_suffixed > 0.5 and j_suffixed > j_base:
        lines = sum(1 for _ in open(os.path.join(fresh_root, rel), encoding="utf8", errors="ignore"))
        displaced.append((rel, base, j_suffixed, j_base, lines))

displaced.sort(key=lambda t: -t[4])
print(f"suffixed files in fresh tree            {suffixed_total:>6,}")
print(f"DISPLACED (prior module pushed to -N)   {len(displaced):>6,}")
print(f"lines held by displaced modules         {sum(d[4] for d in displaced):>6,}\n")
for rel, base, js, jb, lines in displaced[:15]:
    print(f"  {lines:>6,} ln  {rel}")
    print(f"           holds the module that was {base}  (export overlap {js:.0%}, new occupant {jb:.0%})")
