"""Sum score_claims.py results over the held-out apps (lane "indexv2").

usage: python3 -I agg_heldout.py <out dir>/heldout  -> prints the totals per rule and scope, and every wrong-family /
app sample (for reading by hand); writes <dir>/summary.json
"""
import collections, glob, json, os, sys

d = sys.argv[1]
tot = collections.defaultdict(collections.Counter)
lib = collections.Counter()
samples = collections.defaultdict(list)
apps = 0
for f in sorted(glob.glob(d + "/*.score.json")):
    r = json.load(open(f))
    apps += 1
    slug = os.path.basename(f)[:-len(".score.json")]
    lib.update(r["statementBytes"])
    for rp, row in r["reports"].items():
        rule = rp.rsplit(".", 2)[-2]
        for scope, s in row.items():
            tot[(rule, scope)].update(s["bytes"])
            for x in s["samples"]:
                if x["verdict"] in ("wrongFamily", "app"):
                    samples[(rule, scope)].append(dict(x, app=slug))
out = {"apps": apps, "statementBytes": dict(lib), "rules": {}}
for (rule, scope), c in sorted(tot.items()):
    named = c["right"] + c["family"] + c["wrongFamily"]
    row = {"bytes": dict(c), "recall": round((c["right"] + c["family"]) / max(lib["lib"], 1), 4),
           "rightPackageRecall": round(c["right"] / max(lib["lib"], 1), 4),
           "wrongFamilyShareOfNamed": round(c["wrongFamily"] / max(named, 1), 4),
           "sameFamilyShareOfNamed": round(c["family"] / max(named, 1), 4), "appBytes": c["app"]}
    out["rules"][f"{rule}/{scope}"] = row
    print(rule, scope, {k: v for k, v in row.items() if k != "bytes"}, dict(c))
out["samples"] = {f"{r}/{s}": v for (r, s), v in samples.items()}
json.dump(out, open(d + "/summary.json", "w"), indent=1)
print("apps", apps, "statement bytes", dict(lib))
for (rule, scope), v in sorted(samples.items()):
    agg = collections.Counter((x["verdict"], x["package"], x["truth"], x["rule"]) for x in v for _ in [0])
    byt = collections.Counter()
    for x in v:
        byt[(x["verdict"], x["package"], x["truth"], x["rule"])] += x["bytes"]
    print("==", rule, scope, byt.most_common(20))
