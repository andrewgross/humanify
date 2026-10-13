"""humanify's content pairing (vendor_pairing.rs: score = max(5-token shingle Jaccard, IDF literal Jaccard),
accept >= 0.3 with a 0.1 margin) used as an OPEN-WORLD index lookup, vs the variants of this study.

  PYTHONHASHSEED=0 .venv/bin/python humanify_cmp.py   -> stdout + results/humanify_cmp.json
Margin here = top minus best entry of another package (humanify's own margin is mutual best between two
closed sets, which an index lookup does not have). Rows: cross-config lab queries, full modules.
"""
import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402
import srcsets  # noqa: E402

S = fp.load_spans()
src = fp.srcsha()
st = np.array([r["set"] for r in S])
cfg = np.array([r["cfg"] for r in S])
mode = np.array([str(r["mode"]) for r in S])


def load(tag):
    R = dict(np.load(f"{fp.HERE}/results/sim_{tag}.npz"))
    keys = json.load(open(f"{fp.HERE}/results/index_{tag}.json"))["keys"]
    return R, keys


def evaluate(name, parts, t, margin, gate):
    """parts: list of (R, keys); score = max over parts."""
    n = len(S)
    best = np.full(n, -1.0)
    bestkey = [None] * n
    sec = np.full(n, -1.0)
    oth = np.full(n, -1.0)
    othkey = [None] * n
    inf = np.zeros(n)
    gates = gate if isinstance(gate, (list, tuple)) else [gate] * len(parts)
    g = np.zeros(n, bool)
    for (R, keys), gt in zip(parts, gates):
        ok_g = R["qsize"] >= gt  # each score counts only where ITS OWN informative size passes its gate
        g |= ok_g
        top = np.where(ok_g, R["J_top"], -1)
        o = np.where(ok_g, R["J_other"], -1)
        sp = np.where(ok_g, R["J_secondpkg"], -1)
        ti, oi = R["J_topi"], R["J_otheri"]
        upd = top > best
        for i in np.nonzero(upd)[0]:
            bestkey[i] = keys[ti[i]]
        best = np.where(upd, top, best)
        sec = np.maximum(sec, sp)
        upd = o > oth
        for i in np.nonzero(upd)[0]:
            othkey[i] = keys[oi[i]]
        oth = np.maximum(oth, o)
    acc = (best >= t) & (best - sec >= margin) & g
    lab = (st == "lab") & (cfg != "bun-1.4.0") & (mode != "some")
    ok = np.array([bool(bestkey[i]) and (S[i]["pkg"] == bestkey[i].split("\t")[0]
                                          or src.get(fp.file_key(S[i])) == src.get(bestkey[i])) if lab[i] else False
                   for i in range(n)])
    a = lab & acc
    ow = [i for i in np.nonzero(lab & g & (oth >= t))[0] if not srcsets.genuine(fp.file_key(S[i]), othkey[i])]
    neg = {s: int(((st == s) & acc).sum()) for s in ("cc-neg", "cc-negfn", "lab-app")}
    r = {"variant": name, "t": t, "margin": margin, "gate": gate, "queries": int(lab.sum()), "accepted": int(a.sum()),
         "prec_pkg": round(float(ok[a].mean()), 4), "wrong_pkg": int((a & ~ok).sum()),
         "recall_pkg": round(float((a & ok).sum() / lab.sum()), 4),
         "openworld_spurious": len(ow), "openworld_rate": round(len(ow) / lab.sum(), 5), "app_neg_accepted": neg}
    print(json.dumps(r))
    return r


H5, LIT = load("H5_bun-1.4.0"), load("LIT0_bun-1.4.0")
B5d2, LITd = load("B5d2_bun-1.4.0"), LIT
out = []
for t, m, g in ((0.3, 0.1, 0), (0.5, 0.1, 0), (0.7, 0.1, 0), (0.5, 0.1, 20), (0.7, 0.1, 20)):
    out.append(evaluate("humanify max(H5,LIT)", [H5, LIT], t, m, g))
for t, m in ((0.5, 0.1), (0.7, 0.1)):
    out.append(evaluate("B5d2 (boilerplate-filtered)", [B5d2], t, m, 20))
    out.append(evaluate("LIT alone (literal mass >= 40)", [LIT], t, m, 40))
    out.append(evaluate("max(B5d2, LIT), LIT mass >= 40", [B5d2, LIT], t, m, [20, 40]))
    out.append(evaluate("max(B5d2, LIT), LIT mass >= 80", [B5d2, LIT], t, m, [20, 80]))
json.dump(out, open(f"{fp.HERE}/results/humanify_cmp.json", "w"), indent=1)
