"""False-name growth with index size, per evidence family and corpus (the scale table).
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/growth.py  -> results/growth.json + printed markdown
Rows: detector configuration; columns: scale (roots / indexed packages); cell: foreign spans named (rate) on the
FIXED foreign set (packages in no index at any scale), and recall on in-index positives.
"""
import json, pickle
from lib import *
from evalq import metrics, summary, idx_pkgs
from claims import *
from combine import components, variants

WMC = pickle.load(open(D + "wm-claims.pkl", "rb"))
DET = {
    "WM exact module (normLen>=400, guards)": lambda fam, R: WMC[(fam, R, 400, 1, "all")],
    "WM exact module, no stop-lists, no family": lambda fam, R: WMC[(fam, R, 400, 0, "")],
    "UV units k=1, >=20 nodes (original rule)": lambda fam, R: uv_claims(uv_load(fam, R, 20, 0, False, ""), 1),
    "UV units k=2, >=30 nodes, >=3 content (SYNTHESIS §4)": lambda fam, R: uv_claims(uv_load(fam, R, 30, 3, False, ""), 2),
    "UV §4 + stop-lists + families": lambda fam, R: uv_claims(uv_load(fam, R, 30, 3, True, "helpers+runtime+stainless+shim"), 2),
    "UV §4 + stop-lists + families + specificity": lambda fam, R: uv_claims(uv_load(fam, R, 30, 3, True, "helpers+runtime+stainless+shim+spec"), 2),
    "JC J>=0.7, no filter": lambda fam, R: jc_claims(fam, R, 0, 0.7, 0.1, 20),
    "JC J>=0.7, filter d=2 (§4)": lambda fam, R: jc_claims(fam, R, 2, 0.7, 0.1, 20),
    "JC J>=0.7, filter d=10": lambda fam, R: jc_claims(fam, R, 10, 0.7, 0.1, 20),
    "LT §4 rule, no stop-lists, any file": lambda fam, R: lt_claims(R, 20, 0.9, 10, 6, nostop=True),
    "LT §4 rule + stop-lists, any file": lambda fam, R: lt_claims(R, 20, 0.9, 10, 6),
    "LT §4 rule + stop-lists + specific files": lambda fam, R: lt_claims(R, 20, 0.9, 10, 6, specific=True),
}
COMB = ["S", "L", "S|L", "S&L", "design", "design-loose", "WM|S&L"]
res = []
for R in SCALES:
    for fam in ("bun",):
        comps = components(fam, R)
        V = {}
        for name, f in DET.items():
            try:
                cl = f(fam, R)
            except FileNotFoundError:
                continue
            for corp in (None, ("flab",), ("neg",), ("fresh",)):
                m, ex = metrics(lambda q: cl.get(q["sha"]), R, famq=(fam,), corpora=corp)
                res.append(dict(det=name, R=R, corpus="all" if corp is None else corp[0], pkgs=len(idx_pkgs(R, fam)), **summary(m), nonFamily=m["Fn"] - m["Fcopy"]))
        from evalq import Q
        VV = {q["sha"]: variants(comps, q["sha"]) for q in Q() if q["fam"] == fam}
        for v in COMB:
            for corp in (None, ("flab",), ("neg",), ("fresh",)):
                m, ex = metrics(lambda q: VV.get(q["sha"], {}).get(v), R, famq=(fam,), corpora=corp)
                res.append(dict(det="combined: " + v, R=R, corpus="all" if corp is None else corp[0], pkgs=len(idx_pkgs(R, fam)), **summary(m), nonFamily=m["Fn"] - m["Fcopy"]))
        print("scale", R, "done", flush=True)
json.dump(res, open(S + "results/growth.json", "w"), indent=0)
dets = list(dict.fromkeys(r["det"] for r in res))
print("| detector | " + " | ".join(f"R={R} ({len(idx_pkgs(R, 'bun'))} pkgs)" for R in SCALES) + " |")
print("|---|" + "---|" * len(SCALES))
for d in dets:
    cells = []
    for R in SCALES:
        r = next((x for x in res if x["det"] == d and x["R"] == R and x["corpus"] == "all"), None)
        cells.append(f'{r["foreignNamed"]} ({r["foreignRate"]}) / rec {r["recall"]}' if r else "-")
    print(f"| {d} | " + " | ".join(cells) + " |")
