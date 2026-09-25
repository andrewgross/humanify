#!/usr/bin/env python3
"""Dedupe the WP4.4/4.5 harvest (test/parity/wp445-harvest-hooks.py) into
the committed fixture file the Rust tests replay
(crates/humanify-core/src/naming/passes/fixtures_test.rs).

usage: wp445-harvest-collect.py <harvest.jsonl> <out.json>
"""
import json
import sys

src, dst = sys.argv[1:3]
seen = set()
rows = []
with open(src) as f:
    for line in f:
        if not line.strip():
            continue
        row = json.loads(line)
        key = json.dumps(row, sort_keys=True)
        if key in seen:
            continue
        seen.add(key)
        rows.append(row)
# Stable order: by kind, then first-seen.
order = {}
for r in rows:
    order.setdefault(r["kind"], len(order))
rows.sort(key=lambda r: order[r["kind"]])
with open(dst, "w") as f:
    json.dump({"source": "f7a707d unit tests, instrumented (wp445-harvest-hooks.py)",
               "rows": rows}, f, indent=1, ensure_ascii=False)
    f.write("\n")
counts = {}
for r in rows:
    counts[r["kind"]] = counts.get(r["kind"], 0) + 1
print(len(rows), counts)
