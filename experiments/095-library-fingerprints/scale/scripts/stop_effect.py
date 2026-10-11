"""Stop-list effect at R=2000 (bun slice): each detector with and without its stop-lists (and specificity).
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/stop_effect.py -> results/stop-effect.json"""
import json, pickle
from lib import *
from evalq import metrics
from claims import *
R = 2000
W = pickle.load(open(D + "wm-claims.pkl", "rb"))
cfg = {
    "WM no stop": W[("bun", R, 400, 1, "")], "WM + stop": W[("bun", R, 400, 1, "all")],
    "JC no stop": jc_claims("bun", R, 2, 0.7, 0.1, 20, stop=False), "JC + stop": jc_claims("bun", R, 2, 0.7, 0.1, 20),
    "UV k2 no stop": uv_claims(uv_load("bun", R, 30, 3, True, ""), 2), "UV k2 + stop": uv_claims(uv_load("bun", R, 30, 3, True, "helpers+runtime+stainless+shim"), 2),
    "UV k2 + stop + specificity": uv_claims(uv_load("bun", R, 30, 3, True, "helpers+runtime+stainless+shim+spec"), 2),
    "LT any file, no stop": lt_claims(R, nostop=True), "LT any file + stop": lt_claims(R),
    "LT specific, no stop": lt_claims(R, specific=True, nostop=True), "LT specific + stop": lt_claims(R, specific=True),
}
out = []
for k, cl in cfg.items():
    m, ex = metrics(lambda q: cl.get(q["sha"]), R, famq=("bun",))
    r = dict(config=k, foreignNamed=m["Fn"], falseNonFamily=m["Fn"] - m["Fcopy"], recall=pct(m["Pright"] + m["Pfam"], m["P"]), wrongPkg=m["Pwrong"])
    out.append(r); print(json.dumps(r), flush=True)
json.dump(out, open(S + "results/stop-effect.json", "w"), indent=0)
