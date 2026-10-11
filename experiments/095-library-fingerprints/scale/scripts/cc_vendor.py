"""Claude Code 2.1.216 vendor files: what the final detector would name, vs the current pipeline's names.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/cc_vendor.py [R=2000] [variant=design]
The 1,647 CJS factories of the raw bundle (lab/realworld/layout-2.1.216.json, raw order) are the vendor files of the
humanified tree (/tmp/eval-work/candidate-e214db7a-scratch/2.1.216/vendor/_bun-modules.json, manifest order == raw
order for 2.1.216, checked by the lab on its 214 declared positives).
-> results/cc-vendor-<variant>-R<R>.json : every factory with a claim, its current name / nameSource, and the verdict
   agree (same package) | family (same family) | DIFFERENT | NEW (current name is a lib_<hash> fallback)
-> results/cc-vendor-<variant>-R<R>.tsv  : the DIFFERENT / NEW rows for hand review (+ 200-char head of the raw factory)
"""
import json, pickle, sys, collections, re
from lib import *
from evalq import Q

R = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
VAR = sys.argv[2] if len(sys.argv) > 2 else "design"
TREE = "/tmp/eval-work/candidate-e214db7a-scratch/2.1.216/"
man = json.load(open(TREE + "vendor/_bun-modules.json"))["factories"]
V = pickle.load(open(D + f"combine-bun-R{R}.pkl", "rb"))
code = open("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", encoding="utf8").read()
rows = []
cnt = collections.Counter()
for q in Q():
    if q["corpus"] != "cc216" or q["kind"] != "cjs":
        continue
    i = q["cjsIndex"]
    m = man[i]
    v = V.get(q["sha"], {})
    c = v.get(VAR)
    cnt["factories"] += 1
    if not c:
        continue
    cur = m["name"]
    if c == cur:
        verdict = "agree"
    elif m["nameSource"] == "fallback" or re.match(r"^lib_[0-9a-f]{8}", cur):
        verdict = "NEW"
    elif same(c, cur) or cur.startswith(c + "/") or c.startswith(cur + "/"):
        verdict = "family"
    else:
        verdict = "DIFFERENT"
    cnt[verdict] += 1
    cnt[verdict + ":" + m["nameSource"]] += 1
    a, b = q["slice"]
    rows.append(dict(cjsIndex=i, fileName=m["fileName"], current=cur, nameSource=m["nameSource"], claim=c, verdict=verdict,
                     bytes=q["bytes"], evidence={k: v.get(k) for k in ("WM", "JC", "UV", "L", "Rd")},
                     identity=m.get("identity"), head=code[a:b][:200]))
json.dump(dict(R=R, variant=VAR, counts=cnt, rows=rows), open(S + f"results/cc-vendor-{VAR}-R{R}.json", "w"), indent=0)
with open(S + f"results/cc-vendor-{VAR}-R{R}.tsv", "w") as f:
    f.write("cjsIndex\tverdict\tcurrent\tnameSource\tclaim\tbytes\tevidence\thead\n")
    for r in rows:
        if r["verdict"] in ("DIFFERENT", "NEW"):
            ev = ",".join(f"{k}={x}" for k, x in r["evidence"].items() if x)
            f.write(f'{r["cjsIndex"]}\t{r["verdict"]}\t{r["current"]}\t{r["nameSource"]}\t{r["claim"]}\t{r["bytes"]}\t{ev}\t{json.dumps(r["head"])}\n')
print(json.dumps(cnt, indent=0))
