#!/usr/bin/env python3
"""exp190: how many store shards were fingerprinted from a bundle whose Bun
CommonJS wrapper was NOT cut into statements (the layout's wrapper gate
needs >= 50 bindings in the wrapper scope, modules/wrapper.rs). Then the
whole bundle is ONE statement unit: its print, the Bun runtime helpers and
the lab app's own functions are recorded as rows, and every function inside
inherits the ONE owner and file the sourcemap gives the whole bundle.

Evidence used: a row with the lab app function's own print
(`function labAppMain_<tag>(x){...}`, hash LAB below — the same for both
entries, strings are blurred): the lab app is a function of the entry, so it
gets a row only when it sits inside a statement the sourcemap credited to a
package, i.e. inside an uncut wrapper. Sample: every K-th shard by name. Usage: uncut_wrappers.py <shard-dir> <K>"""
import json, os, sys

LAB = "b561d6b0aed66d93"

d, k = sys.argv[1], int(sys.argv[2])
files = sorted(os.listdir(d))[::k]
stats = {"shards": 0, "withUnits": 0, "uncut": 0, "rows": 0, "uncutRows": 0, "uncutOwnerIsPackage": 0, "uncutOwnerIsDep": 0}
examples = []
for f in files:
    try:
        s = json.load(open(os.path.join(d, f)))
    except ValueError:
        continue
    stats["shards"] += 1
    units = s.get("units", [])
    if not units:
        continue
    stats["withUnits"] += 1
    stats["rows"] += len(units)
    lab = [u for u in units if u["hash"] == LAB]
    if lab:
        stats["uncut"] += 1
        stats["uncutRows"] += len(units)
        owners = {u["owner"] for u in lab}
        if s["package"] in owners:
            stats["uncutOwnerIsPackage"] += 1
        if owners - {s["package"]}:
            stats["uncutOwnerIsDep"] += 1
            if len(examples) < 15:
                examples.append((f, sorted(owners)))
print(json.dumps(stats, indent=1))
for e in examples:
    print(*e)
