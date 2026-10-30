#!/usr/bin/env python3
"""exp166: sum one label's per-bundle vendor-placement precision cards
(vendor_precision.py) per world (exact / ordinal) and for the factories.
usage: python3 -I aggregate_precision.py <label-dir>   # prints JSON"""
import collections
import glob
import json
import os
import sys

out = {"bundles": 0, "noBridge": [], "noCard": [], "lists": {}}
sums = {k: collections.Counter() for k in ("exact", "ordinal", "factory")}
for path in sorted(glob.glob(os.path.join(sys.argv[1], "*.precision.json"))):
    bid = os.path.basename(path)[: -len(".precision.json")]
    if os.path.getsize(path) == 0:
        out["noCard"].append(bid)
        continue
    card = json.load(open(path))
    if card.get("noBridge"):
        out["noBridge"].append(bid)
        continue
    out["bundles"] += 1
    for world, counts in sums.items():
        for key, value in card[world].items():
            if isinstance(value, list):
                counts[key] += len(value)
                if value and key.endswith(("appFiles", "wrongFiles")):
                    out["lists"].setdefault(f"{world}:{key}", {})[bid] = value
            else:
                counts[key] += value
out.update({k: dict(sorted(v.items())) for k, v in sums.items()})
print(json.dumps(out, indent=1, sort_keys=True))
