"""Experiment A step 3b: the greedy order again, with the cost a package REALLY adds to a humanify-index index.

usage: python3 -I greedy_ix.py <index raw MB>...   -> $SEL/lists/greedyix-<MB>.txt, $SEL/greedy-ix.json

Step 4 showed the code-byte cost model is wrong for the real index: the literal table (97% of an index) holds
every source file of every installed package at up to 4 versions, so an owner's index bytes range 0.9-11.7x its
bundled code (p10-p90; caniuse-lite 43 MB for 0.3 MB of code). Here c(p) = p's measured index bytes (every row
owned by p in the greedy-20 and download-30 indexes, deduplicated), and for a package in neither index its code
cost x the measured median (3.4). Same training apps, same gain, same static ratio order.
"""
import json, os, sys, collections, statistics

SEL = os.environ.get("SEL", "/work/exp095-library-fingerprints/selection")
sel = json.load(open(SEL + "/selection.json"))
if not os.path.exists(SEL + "/owner-index-bytes.json"):
    ob, seen = collections.Counter(), set()
    for d in ("greedy-20", "download-30"):
        for f in ("literals.jsonl", "modules.bun.jsonl"):
            for l in open(f"{SEL}/ix/{d}/{f}"):
                if (f, l) not in seen:
                    seen.add((f, l))
                    ob[json.loads(l)["owner"]] += len(l)
    json.dump(dict(ob), open(SEL + "/owner-index-bytes.json", "w"))
ob = json.load(open(SEL + "/owner-index-bytes.json"))
code = sel["cost"]
ratio = statistics.median(ob[o] / code[o] for o in ob if code.get(o, 0) > 1000)
cost = {n: ob.get(n) or code[n] * ratio for n in code if code[n] > 0}
held = set(sel["heldout"])
gain = collections.Counter()
for f in os.listdir(SEL + "/apps"):
    p = f"{SEL}/apps/{f}/closure.json"
    if not os.path.exists(p):
        continue
    r = json.load(open(p))
    if r["id"] in held or not r.get("packages"):
        continue
    for n, x in r["packages"].items():
        gain[n] += x["bytes"]
order = sorted((n for n in gain if cost.get(n, 0) > 0), key=lambda n: (-gain[n] / cost[n], n))
for b in [float(x) for x in sys.argv[1:]]:
    out, spent = [], 0.0
    for n in order:
        if spent + cost[n] > b * 1e6:
            break
        out.append(n)
        spent += cost[n]
    open(f"{SEL}/lists/greedyix-{b:g}.txt", "w").write("\n".join(out) + "\n")
    print("greedyix", b, len(out), round(spent / 1e6, 1))
json.dump({"ratio": ratio, "order": order[:3000]}, open(SEL + "/greedy-ix.json", "w"))
