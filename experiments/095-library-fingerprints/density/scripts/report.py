"""Print the density tables (markdown) from results/density-<slice>.json and results/density-cc.json.
usage: python3 -I report.py [bun|esbuild]
"""
import json, os, statistics, sys, collections

DEN = os.environ.get("DEN", "/work/exp095-library-fingerprints/density")
SL = sys.argv[1] if len(sys.argv) > 1 else "bun"
r = json.load(open(f"{DEN}/results/density-{SL}.json"))
P = r["packages"]
POL = ["all", "patch/2", "patch/5", "minor", "year-all+minor", "minor-last2+major", "major", "asof-default", "latest"]
SC = ["exact", "nearest-patch", "same-minor", "latest-before", "same-major", "other-major"]
mean = lambda xs: statistics.mean([x for x in xs if x is not None]) if [x for x in xs if x is not None] else float("nan")
print(f"slice {SL}: {len(P)} packages, {sum(p['versions'] for p in P.values())} versions "
      f"({sum(p['queried'] for p in P.values())} with query modules), {r['distinctQueries']} distinct query modules, "
      f"index rows {r['rows']} literal / {r['owners']} owners\n")
print("| policy | versions kept | module rows | literal rows | literal MB | rows if not deduped (mod / lit) | recall (uniform) | recall (download-weighted) | wrong (uniform / dl) |")
print("|---|---|---|---|---|---|---|---|---|")
for k in POL:
    xs = [p["policies"][k] for p in P.values() if "recall" in p["policies"].get(k, {})]
    if not xs:
        continue
    s = lambda f: sum(p["policies"][k][f] for p in P.values() if k in p["policies"])
    print(f"| {k} | {s('kept'):,} | {s('moduleRows'):,} | {s('literalRows'):,} | {s('literalBytes') / 1e6:.1f} | "
          f"{s('moduleRowsNoDedup'):,} / {s('literalRowsNoDedup'):,} | {100 * mean([x['recall'] for x in xs]):.1f}% | "
          f"{100 * mean([x['recallDl'] for x in xs]):.1f}% | {100 * mean([x['wrong'] for x in xs]):.2f}% / {100 * mean([x['wrongDl'] for x in xs]):.2f}% |")
print("\n| scenario (X itself never indexed, except exact) | packages | recall (uniform) | recall (download-weighted) | wrong |")
print("|---|---|---|---|---|")
for k in SC:
    xs = [p["scenarios"][k] for p in P.values() if k in p["scenarios"]]
    print(f"| {k} | {len(xs)} | {100 * mean([x['recall'] for x in xs]):.1f}% | {100 * mean([x['recallDl'] for x in xs]):.1f}% | {100 * mean([x['wrong'] for x in xs]):.2f}% |")
# single-file vs multi-file packages: how the nearest patch does
print("\nnearest-patch recall by package (uniform over versions that have a neighbour patch), lowest 15:",
      sorted(((round(p["scenarios"]["nearest-patch"]["recall"], 2), n) for n, p in P.items() if "nearest-patch" in p["scenarios"]))[:15])
if SL == "bun" and os.path.exists(f"{DEN}/results/density-cc.json"):
    c = json.load(open(f"{DEN}/results/density-cc.json"))
    rows = list(c["rows"].values())
    print(f"\nClaude Code: {c['modules']} factory modules over the CC versions, {c['namedDensity']} named a density package "
          f"with every version indexed; {len(rows)} (package, CC version) pairs, "
          f"{sum(1 for x in rows if x['how'] == 'declared')} with a declared version")
    print("\n| policy / scenario | pairs | recall, bytes (relative to all versions) | wrong |")
    print("|---|---|---|---|")
    for k in POL:
        xs = [x["policies"][k] for x in rows if k in x["policies"]]
        b = [x["bytes"] for x in rows if k in x["policies"]]
        if xs:
            print(f"| {k} | {len(xs)} | {100 * sum(a['recall'] * w for a, w in zip(xs, b)) / sum(b):.1f}% | {100 * sum(a['wrong'] * w for a, w in zip(xs, b)) / sum(b):.2f}% |")
    for k in SC:
        xs = [x["scenarios"][k] for x in rows if k in x["scenarios"]]
        b = [x["bytes"] for x in rows if k in x["scenarios"]]
        if xs:
            print(f"| scenario {k} | {len(xs)} | {100 * sum(a['recall'] * w for a, w in zip(xs, b)) / sum(b):.1f}% | {100 * sum(a['wrong'] * w for a, w in zip(xs, b)) / sum(b):.2f}% |")
    dec = [x for x in rows if x["how"] == "declared"]
    print("\ndeclared pairs:", [(x["p"], x["cc"], x["X"], x["policies"].get("minor", {}).get("recall"), x["policies"].get("asof-default", {}).get("recall")) for x in dec])
