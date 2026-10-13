"""The negatives lane's hand-labelled Claude Code 2.1.216 src/ statements (random stratified sample S0-S3 + review
queue + traps) against the detectors at scale R.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/ccsrc_eval.py [R=2000]  -> results/ccsrc-R<R>.json
Two configurations:
  vendor   the module-level components exactly as combine.py (literal specificity on: a statement is rarely a whole file)
  stepb    statement-level: literal specificity OFF, UV k>=1 without the module-size specificity, same stop-lists
For each: false claims on units labelled app (incl. app forks), on traps, and recall on units labelled library.
"""
import json, pickle, sys, collections
from lib import *
from evalq import Q
from claims import jc_claims, lt_claims, uv_load, uv_claims
from combine import consensus, agree, RD, WMC

R = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
STOP = "helpers+runtime+stainless+shim"
fam = "bun"
comp = {
    "vendor": dict(WM=WMC[(fam, R, 400, 1, "all")], JC=jc_claims(fam, R, 2, 0.7, 0.1, 20), UV=uv_claims(uv_load(fam, R, 30, 3, True, STOP + "+spec"), 2),
                   L=lt_claims(R, 20, 0.9, 10, 6, specific=True)),
    "stepb": dict(WM=WMC[(fam, R, 400, 1, "all")], JC=jc_claims(fam, R, 2, 0.7, 0.1, 20), UV=uv_claims(uv_load(fam, R, 30, 3, True, STOP), 1),
                  L=lt_claims(R, 20, 0.9, 10, 6, specific=False)),
}
rd = {s: p for s, (p, h) in RD.items()}
out = {}
for cname, c in comp.items():
    m = collections.Counter()
    ex = collections.defaultdict(list)
    for q in Q():
        if q["corpus"] != "ccsrc":
            continue
        s = q["sha"]
        S_ = consensus(c["WM"].get(s), c["JC"].get(s), c["UV"].get(s))
        L_ = c["L"].get(s)
        r_ = rd.get(s)
        claims = {"S": S_, "L": L_, "S|L": consensus(S_, L_), "S&L": agree(S_, L_, L_),
                  "design": agree(S_, L_, L_) or agree(S_, r_, r_) or agree(L_, r_, r_)}
        lab = q["label"]
        grp = "trap" if q.get("trap") else ("queue" if q.get("stratum") == "review-queue" or str(q["idx"]).startswith("q") else "sample")
        kind = "lib" if lab.startswith("library:") else ("app" if lab == "app" else lab)
        for k, v in claims.items():
            key = f"{grp}:{kind}:{k}"
            m[f"{grp}:{kind}"] += 0
            if v:
                m[key] += 1
                m[key + ":bytes"] += q["bytes"]
                if kind == "lib":
                    if same(v, q["pkg"]):
                        m[key + ":right"] += 1
                        m[key + ":rightBytes"] += q["bytes"]
                    else:
                        ex[key + ":wrong"].append((q["idx"], q["pkg"], v, q["bytes"]))
                else:
                    ex[key].append((q["idx"], lab, q.get("fork_of"), q.get("trap"), v, q["bytes"]))
        m[f"{grp}:{kind}:n"] += 1
        m[f"{grp}:{kind}:nbytes"] += q["bytes"]
    out[cname] = dict(counts=m, examples=ex)
    print("==", cname)
    for grp in ("sample", "queue", "trap"):
        for kind in ("app", "lib", "compiler-helper", "bundler-runtime"):
            n = m[f"{grp}:{kind}:n"]
            if not n:
                continue
            line = f"  {grp:6s} {kind:16s} n={n:4d} ({m[f'{grp}:{kind}:nbytes'] / 1024:.0f} KB)"
            for k in ("S", "L", "S|L", "S&L", "design"):
                key = f"{grp}:{kind}:{k}"
                if kind == "lib":
                    line += f" | {k} claimed {m[key]} right {m[key + ':right']} ({m[key + ':rightBytes'] / max(m[f'{grp}:{kind}:nbytes'], 1) * 100:.1f}% B)"
                else:
                    line += f" | {k} {m[key]} ({m[key + ':bytes']} B)"
            print(line)
json.dump(out, open(S + f"results/ccsrc-R{R}.json", "w"), indent=0, default=list)
