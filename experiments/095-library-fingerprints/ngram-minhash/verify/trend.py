"""Spurious (non-genuine) open-world calls per LOPO run, by gate and threshold; plus the tail of the best
other-package score. PYTHONHASHSEED=0 ../.venv/bin/python trend.py <lopo tags...>"""
import json, os, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, os.path.dirname(HERE))
import fp, srcsets  # noqa
S = fp.load_spans()
keys = json.load(open(f"{HERE}/results/lopo_keys_B5_bun-1.4.0.json"))
cache = {}
def spur(qi, e):
    k = (fp.file_key(S[qi]), keys[e])
    if k not in cache: cache[k] = not srcsets.genuine(*k)
    return cache[k]
res = {}
for tag in sys.argv[1:]:
    L = dict(np.load(f"{HERE}/results/lopo_{tag}.npz")); q = L["qidx"]
    lab = np.array([S[i]["set"] == "lab" for i in q])
    row = {}
    for gate in (0, 10, 20):
        g = lab & (L["qsize"] >= max(gate, 1e-9))
        for M in ("J", "CQ"):
            for t in (0.3, 0.4, 0.5, 0.7, 0.9):
                idx = np.nonzero(g & (L[M] >= t))[0]
                row[f"g{gate}_{M}{t}"] = [int(g.sum()), int(sum(spur(q[j], L[M + 'i'][j]) for j in idx))]
    res[tag] = row
    print(tag, {k: v for k, v in row.items() if k.startswith("g20") or k in ("g0_J0.7", "g0_J0.5", "g0_CQ0.7")})
json.dump(res, open(f"{HERE}/results/trend.json", "w"), indent=1)
