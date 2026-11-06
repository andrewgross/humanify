#!/usr/bin/env python3
"""exp190 (b): pick ~50 package@versions from the exp184 store by download rank
(npm-high-impact `top` order) across size classes. Reads the store's
bun-bun-cjs-min shards; writes a TSV: rank pkg version class units nodes files.

Size class by the package's OWN distinct unit prints in the shard
(tiny < 15, medium 15-299, large >= 300)."""
import json, os, re, sys

STORE = sys.argv[1]  # <store>/shards/bun-bun-cjs-min
TOP = sys.argv[2]    # top.list
PER_CLASS = int(sys.argv[3]) if len(sys.argv) > 3 else 17
names = re.findall(r"'([^']+)'", open(TOP).read())
by_pkg = {}
for f in os.listdir(STORE):
    m = re.match(r"^(.+)@([^@]+)\.json$", f)
    if m:
        by_pkg.setdefault(m.group(1).replace("__", "/", 1) if m.group(1).startswith("@") else m.group(1), []).append(m.group(2))

def key(v):
    if "-" in v:
        return None
    try:
        return tuple(int(x) for x in v.split("."))
    except ValueError:
        return None

def cls(n):
    return "tiny" if n < 15 else ("medium" if n < 300 else "large")

picked = {"tiny": [], "medium": [], "large": []}
for rank, name in enumerate(names, 1):
    if all(len(v) >= PER_CLASS for v in picked.values()):
        break
    vs = [v for v in by_pkg.get(name, []) if key(v)]
    if not vs:
        continue
    v = max(vs, key=key)
    shard = json.load(open(os.path.join(STORE, f"{name.replace('/', '__')}@{v}.json")))
    own = {}
    files = set()
    for u in shard["units"]:
        if u["owner"] == name:
            own[u["hash"]] = u["nodes"]
            files.add(u["file"])
    if not own:
        continue
    c = cls(len(own))
    if len(picked[c]) < PER_CLASS:
        picked[c].append((rank, name, v, c, len(own), sum(own.values()), len(files)))
for c in picked.values():
    for row in c:
        print("\t".join(map(str, row)))
