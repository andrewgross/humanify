#!/usr/bin/env python3
"""exp190: how many DISTINCT owner packages each unit hash is recorded under,
over a deterministic sample of store shards (every K-th file by name). A
print that dozens of unrelated packages "own" is not theirs: Bun's runtime
helpers (__commonJS, __toESM, __export, ...) and the lab app, attributed to
the package by the sourcemap. Usage: shared_prints.py <shard-dir> <K> <out.json>"""
import json, os, sys

d, k, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
owners = {}
files = sorted(os.listdir(d))[::k]
for f in files:
    try:
        s = json.load(open(os.path.join(d, f)))
    except ValueError:
        continue
    for u in s.get("units", []):
        if u["nodes"] >= 20:
            owners.setdefault(u["hash"], set()).add(u["owner"])
json.dump({"sampledShards": len(files), "owners": {h: len(o) for h, o in owners.items() if len(o) >= 3}}, open(out, "w"))
print(len(files), sum(1 for o in owners.values() if len(o) >= 10))
