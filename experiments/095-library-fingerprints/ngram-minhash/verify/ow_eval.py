"""Summarise lopo_*.npz: open-world false-library calls (genuine copies excused by srcsets.genuine) vs the
original sim's (filter computed with the own package present).
  PYTHONHASHSEED=0 ../.venv/bin/python ow_eval.py <lopo tag> [orig sim tag]
"""
import json, os, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
import fp, srcsets  # noqa
tag = sys.argv[1]
orig = sys.argv[2] if len(sys.argv) > 2 else None
L = dict(np.load(f"{HERE}/results/lopo_{tag}.npz"))
stream_n, cfg = tag.split("_")[0], tag.split("_")[1]
keys = json.load(open(f"{HERE}/results/lopo_keys_{stream_n.split('d')[0]}_{cfg}.json"))
S = fp.load_spans()
q = L["qidx"]
st = np.array([S[i]["set"] for i in q]); qcfg = np.array([S[i]["cfg"] for i in q])
lab = st == "lab"
cache = {}
def spur(j, eidx):
    k = (fp.file_key(S[q[j]]), keys[eidx])
    if k not in cache:
        cache[k] = not srcsets.genuine(*k)
    return cache[k]
out = {"tag": tag}
for gate in (10, 20, 30, 50):
    for M in ("J", "CQ"):
        for t in (0.5, 0.7, 0.8, 0.9):
            m = lab & (L["qsize"] >= gate) & (L[M] >= t)
            idx = np.nonzero(m)[0]
            sp_ = [j for j in idx if spur(j, L[M + "i"][j])]
            byc = {c: int(sum(qcfg[j] == c for j in sp_)) for c in sorted(set(qcfg[lab]))}
            ng = int((lab & (L["qsize"] >= gate)).sum())
            # with the J margin 0.1 against the next package (what a shipped rule would do)
            spm = [j for j in sp_ if M == "J" and L["J"][j] - L["J2"][j] >= 0.1]
            out[f"g{gate}_{M}{t}"] = {"gated": ng, "hits": len(idx), "spurious": len(sp_), "spurious_with_margin": len(spm) if M == "J" else None, "by_cfg": byc}
            if gate == 20 and t == 0.7:
                out[f"examples_g20_{M}0.7"] = [{"q": fp.file_key(S[q[j]]), "cfg": qcfg[j], "qsize": float(L["qsize"][j]),
                                                "score": float(L[M][j]), "hit": keys[L[M + "i"][j]]} for j in sp_[:25]]
for g in ("cc-neg", "cc-negfn", "lab-app"):
    m = st == g
    out[g] = {t: int((m & (L["qsize"] >= 20) & (L["J"] >= t)).sum()) for t in (0.5, 0.7)} | {"CQ0.7": int((m & (L["qsize"] >= 20) & (L["CQ"] >= 0.7)).sum())}
if orig:
    R = dict(np.load(f"{os.path.dirname(HERE)}/results/sim_{orig}.npz"))
    ok = keys == json.load(open(f"{os.path.dirname(HERE)}/results/index_{orig}.json"))["keys"]
    oq, oJ, oi = R["qsize"][q], R["J_other"][q], R["J_otheri"][q]
    for t in (0.5, 0.7):
        m = lab & (oq >= 20) & (oJ >= t)
        out[f"ORIG_g20_J{t}"] = {"hits": int(m.sum()), "spurious": int(sum(spur(j, oi[j]) for j in np.nonzero(m)[0]))}
    out["gated_orig_vs_lopo"] = [int((lab & (oq >= 20)).sum()), int((lab & (L["qsize"] >= 20)).sum())]
json.dump(out, open(f"{HERE}/results/oweval_{tag}.json", "w"), indent=1)
for k, v in out.items():
    if not k.startswith("examples"):
        print(k, v)
for e in out.get("examples_g20_J0.7", [])[:25]:
    print(e)
