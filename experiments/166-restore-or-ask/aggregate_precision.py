#!/usr/bin/env python3
"""exp166: sum the per-bundle vendor-placement precision cards of one label.
usage: python3 -I aggregate_precision.py <label-dir>   # prints JSON"""
import collections
import glob
import json
import os
import sys

out = {"bundles": 0, "noBridge": 0, "placement": collections.Counter(),
       "conversion": collections.Counter(), "appFiles": {}, "wrongFiles": {}}
for path in sorted(glob.glob(os.path.join(sys.argv[1], "*.precision.json"))):
    card = json.load(open(path))
    bid = os.path.basename(path)[: -len(".precision.json")]
    out["bundles"] += 1
    out["placement"].update(card["placement"])
    out["conversion"].update(card["conversion"])
    if card["appFiles"]:
        out["appFiles"][bid] = card["appFiles"]
    if card["wrongFiles"]:
        out["wrongFiles"][bid] = card["wrongFiles"]
out["noBridge"] = out["placement"].pop("noBridge", 0)
print(json.dumps(out, indent=1, sort_keys=True))
