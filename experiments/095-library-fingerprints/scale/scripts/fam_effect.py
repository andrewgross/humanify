"""Effect of the package families on how many foreign names count as false, and on recall, at R=2000.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/fam_effect.py  -> results/family-effect.json
The claims are fixed (as combine.py makes them); only the judgement changes:
  none  = only the exact package is right; brief = the five SYNTHESIS §4 families; full = brief + same npm scope +
  unscoped/scoped forks of the same bare name.
"""
import json, pickle
import lib
from lib import *
from evalq import metrics

R = 2000
V = pickle.load(open(D + f"combine-bun-R{R}.pkl", "rb"))
out = []
for mode in ("none", "brief", "full"):
    lib.FAMILY_MODE = mode
    lib._FX = None
    for name in ("WM", "JC", "UV", "S", "L", "S&L", "design"):
        m, ex = metrics(lambda q: V.get(q["sha"], {}).get(name), R, famq=("bun",))
        r = dict(mode=mode, variant=name, foreignNamed=m["Fn"], falseNonFamily=m["Fn"] - m["Fcopy"], familyRight=m["Fcopy"],
                 recall=pct(m["Pright"] + m["Pfam"], m["P"]), recallStrict=pct(m["Pright"], m["P"]), wrongPkg=m["Pwrong"])
        out.append(r)
        print(json.dumps(r))
json.dump(out, open(S + "results/family-effect.json", "w"), indent=0)
