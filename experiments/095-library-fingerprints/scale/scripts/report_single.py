"""Single-family results across scales (the false-name growth table) and threshold sweeps.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/report_single.py [jc|lt|uv|wm|all]
-> results/single-<family>.json and printed tables"""
import json, sys, os
from lib import *
from evalq import metrics, summary
from claims import *

os.makedirs(S + "results", exist_ok=True)
what = sys.argv[1] if len(sys.argv) > 1 else "all"
Rs = [int(x) for x in sys.argv[2].split(",")] if len(sys.argv) > 2 else SCALES


def run(label, fam, R, cl):
    m, ex = metrics(lambda q: cl.get(q["sha"]), R, famq=(fam,))
    r = dict(detector=label, fam=fam, R=R, **summary(m))
    print(json.dumps(r), flush=True)
    return r, ex


out = []
if what in ("jc", "all"):
    for fam in ("bun", "esbuild"):
        for R in Rs:
            for d in (0, 2, 5, 10):
                if not os.path.exists(D + f"jc-{fam}-R{R}-d{d}.npz"):
                    continue
                for J, g in ((0.7, 20), (0.5, 20), (0.6, 20), (0.8, 20), (0.9, 20), (0.7, 10), (0.7, 50)):
                    if (J, g) != (0.7, 20) and (d != 2 or R != max(Rs)):
                        continue
                    r, ex = run(f"jc d={d} J>={J} m0.1 gate{g}", fam, R, jc_claims(fam, R, d, J, 0.1, g))
                    r["examples"] = ex["F"][:40] + [("APP",) + x for x in ex["A"][:20]]
                    out.append(r)
    json.dump(out, open(S + "results/single-jc.json", "w"), indent=0)
if what in ("lt", "all"):
    out = []
    for R in Rs:
        for (T, C, M, n, fmg, spc) in ((20, 0.9, 10, 6, False, False), (20, 0.9, 10, 6, False, True), (20, 0.9, 10, 6, True, True), (10, 0.9, 10, 6, False, True),
                                       (40, 0.9, 10, 6, False, True), (20, 0.7, 10, 6, False, True), (20, 0.8, 10, 6, False, True), (20, 0.9, 20, 6, False, True),
                                       (20, 0.9, 5, 6, False, True), (30, 0.9, 10, 10, False, True), (60, 0.9, 10, 6, False, True), (20, 0.95, 10, 6, False, True)):
            if (T, C, M, n, fmg) != (20, 0.9, 10, 6, False) and R != max(Rs):
                continue
            for fam in ("bun", "esbuild"):
                r, ex = run(f"lt T{T} C{C} M{M} n>{n - 1}{' famMargin' if fmg else ''}{' specific' if spc else ''}", fam, R, lt_claims(R, T, C, M, n, family_margin=fmg, specific=spc))
                r["examples"] = ex["F"][:60] + [("APP",) + x for x in ex["A"][:20]]
                out.append(r)
    json.dump(out, open(S + "results/single-lt.json", "w"), indent=0)
if what in ("uv", "all"):
    out = []
    CF = ["20,0,0,", "30,3,0,", "30,3,1,", "30,3,1,helpers+runtime+stainless+shim", "30,3,1,helpers+runtime+stainless+shim+spec", "30,3,0,helpers+runtime+stainless+shim+spec"]
    for fam in ("bun", "esbuild"):
        for R in Rs:
            for c in CF:
                nmin, cmin, fm, stop = c.split(",")
                try:
                    v = uv_load(fam, R, nmin, cmin, fm == "1", stop)
                except FileNotFoundError:
                    continue
                for k in (1, 2, 3):
                    r, ex = run(f"uv n{nmin} c{cmin} fam{fm} stop={stop or '-'} k{k}", fam, R, uv_claims(v, k))
                    r["examples"] = ex["F"][:80] + [("APP",) + x for x in ex["A"][:20]] + [("PWRONG",) + x for x in ex["Pwrong"][:30]]
                    out.append(r)
    json.dump(out, open(S + "results/single-uv.json", "w"), indent=0)
if what in ("wm", "all"):
    import pickle
    W = pickle.load(open(D + "wm-claims.pkl", "rb"))
    out = []
    for (fam, R, L, fm, stop), cl in sorted(W.items()):
        if R not in Rs:
            continue
        if (L, fm, stop) != (400, 1, "all") and R != max(Rs):
            continue
        r, ex = run(f"wm normLen>={L} fam{fm} stop={stop or '-'}", fam, R, cl)
        r["examples"] = ex["F"][:60] + [("APP",) + x for x in ex["A"][:20]] + [("PWRONG",) + x for x in ex["Pwrong"][:30]]
        out.append(r)
    json.dump(out, open(S + "results/single-wm.json", "w"), indent=0)
