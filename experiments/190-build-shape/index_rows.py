#!/usr/bin/env python3
"""exp190 (c): pull the cb64 index's unit rows of a few owners out of the
row tables (units.<slice>.jsonl, both Bun slices), with each row's version
ranges expanded over versions.json. Usage:
  index_rows.py <index-dir> <out.json> <owner>...
Writes {owner: {"versions": [...], "rows": [{hash, file, nodes, slice, versions:[...]}]}}."""
import json
import os
import sys

INDEX, OUT = sys.argv[1:3]
OWNERS = set(sys.argv[3:])
versions = json.load(open(os.path.join(INDEX, "versions.json")))
out = {o: {"versions": versions.get(o, []), "rows": []} for o in OWNERS}
for slice_ in ("bun-bun-cjs-min", "bun-node-cjs-min"):
    with open(os.path.join(INDEX, f"units.{slice_}.jsonl")) as f:
        for line in f:
            i = line.find('"owner":"')
            if i < 0:
                continue
            owner = line[i + 9 : line.index('"', i + 9)]
            if owner not in OWNERS:
                continue
            r = json.loads(line)
            order = versions[owner]
            expanded = []
            for a, b in r["versions"]:
                expanded += order[order.index(a) : order.index(b) + 1]
            out[owner]["rows"].append(
                {"hash": r["hash"], "file": r["file"], "nodes": r["nodes"], "slice": slice_, "versions": expanded}
            )
json.dump(out, open(OUT, "w"))
for o in sorted(OWNERS):
    print(o, len(out[o]["rows"]))
