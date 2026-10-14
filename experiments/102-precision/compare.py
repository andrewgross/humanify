"""exp102: exact byte deltas between exp096 labels, per set: named (true positives), wrong family, app claimed,
P / R / F1. usage: python3 compare.py <base label> <label>..."""
import json
import os
import sys

R = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../096-library-matching-score/results")


def sets(label):
    return json.load(open(os.path.join(R, label, "summary.json")))["sets"]


base = sets(sys.argv[1])
for label in sys.argv[1:]:
    s = sets(label)
    print(label)
    for k in ("heldout", "esbuild-app", "wrangler", "cc"):
        a, b = base[k], s[k]
        print(f"  {k:12s} P {b['precision']*100:6.2f} R {b['recall']*100:6.2f} F1 {b['f1']*100:6.2f}   "
              f"named {b['namedBytes']:>9,} ({b['namedBytes']-a['namedBytes']:+,})  "
              f"wrong {b['wrongFamilyBytes']:>8,} ({b['wrongFamilyBytes']-a['wrongFamilyBytes']:+,})  "
              f"app {b['appClaimedBytes']}")
