#!/usr/bin/env python3
"""exp166: statements that crossed the src/ <-> vendor/ boundary on a hop
(placement trail file vs priorFile), both directions, with bytes.
usage: python3 cross_boundary.py <diag.json>"""
import collections
import json
import sys

d = json.load(open(sys.argv[1]))
out = collections.Counter()
files = collections.defaultdict(set)
for t in d["placementTrails"]["trails"]:
    f, p = t.get("file") or "", t.get("priorFile")
    if not p:
        continue
    a, b = p.split("/")[0], f.split("/")[0]
    if a != b and {a, b} == {"src", "vendor"}:
        key = f"{a}->{b}"
        out[key + ":statements"] += 1
        out[key + ":bytes"] += t["span"]["end"] - t["span"]["start"]
        files[key].add(f)
print(json.dumps({**out, **{k + ":files": sorted(v) for k, v in files.items()}}, indent=1))
