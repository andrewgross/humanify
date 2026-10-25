"""exp150: the blind second pass's units — 20% of the Claude Code sample, uniform at random (seed 9520, exp095's).

usage: python3 -I blind_draw.py <neg dir>     reads <neg dir>/data/sample.jsonl, writes <neg dir>/blind/blind-ids.json
"""
import json
import random
import sys

SEED, SHARE = 9520, 0.2
neg = sys.argv[1]
ids = sorted(json.loads(line)["i"] for line in open(f"{neg}/data/sample.jsonl"))
pick = sorted(random.Random(SEED).sample(ids, round(SHARE * len(ids))))
json.dump({"seed": SEED, "share": SHARE, "of": len(ids), "ids": pick}, open(f"{neg}/blind/blind-ids.json", "w"))
print(len(pick), "blind ids ->", f"{neg}/blind/blind-ids.json")
