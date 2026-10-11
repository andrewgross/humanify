"""The report's module-level tables, bucketed by INFORMATIVE size (the query's shingles that survive the
index's boilerplate filter = sim's weighted qsize), not by raw tokens.

  PYTHONHASHSEED=0 .venv/bin/python tables.py <tag> [metric J|CQ] [margin] [file|pkg]
      -> stdout + results/tables_<tag>_<metric>_<file|pkg>.json

accepted       top >= t AND (top - best entry of a different FILE, other source) >= margin AND informative size >= 20
prec_pkg       accepted whose top entry is the query's package (or byte-identical source)
recall         accepted & file-correct / queries whose file is in the index
ow_spurious    open-world: queries whose best OTHER-package score >= t, size gate passed, and the two source
               files do NOT genuinely share code (srcsets.genuine) -> false-library rate if the package
               were absent from the index
"""
import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402
import srcsets  # noqa: E402

MIN_INF = 20
BUCKETS = [(0, 20), (20, 50), (50, 100), (100, 300), (300, 1000), (1000, 10 ** 9)]
CFGS = ["bun-1.3.14", "bun-1.4.0", "bun-1.4.3", "esbuild-0.28.2", "rollup-4.64.5"]


def main():
    tag = sys.argv[1]
    M = sys.argv[2] if len(sys.argv) > 2 else "J"
    margin = float(sys.argv[3]) if len(sys.argv) > 3 else 0.0
    against = sys.argv[4] if len(sys.argv) > 4 else "file"  # margin against the best other FILE or other PACKAGE
    cfg_index = tag.split("_", 1)[1]
    R = dict(np.load(f"{fp.HERE}/results/sim_{tag}.npz"))
    keys = json.load(open(f"{fp.HERE}/results/index_{tag}.json"))["keys"]
    S = fp.load_spans()
    src = fp.srcsha()
    ek = [k.split("\t") for k in keys]
    esrc = [src.get(k) for k in keys]
    st = np.array([r["set"] for r in S])
    cfg = np.array([r["cfg"] for r in S])
    mode = np.array([str(r["mode"]) for r in S])
    qs = R["qsize"]
    top, topi, sec, oth, othi = R[f"{M}_top"], R[f"{M}_topi"], R[f"{M}_second{against}"], R[f"{M}_other"], R[f"{M}_otheri"]
    qsrc = [src.get(fp.file_key(r)) if r["set"] == "lab" else None for r in S]
    pkg_ok = np.array([bool(S[i]["pkg"] == ek[t][0] or (qsrc[i] and qsrc[i] == esrc[t])) for i, t in enumerate(topi)])
    file_ok = np.array([bool((S[i]["pkg"] == ek[t][0] and S[i]["file"] == ek[t][2]) or (qsrc[i] and qsrc[i] == esrc[t]))
                        for i, t in enumerate(topi)])
    ver_ok = np.array([bool(qsrc[i] and qsrc[i] == esrc[t]) for i, t in enumerate(topi)])
    in_index = (R["J_same"] >= 0) | (R["J_samefile"] >= 0)
    gate = qs >= MIN_INF
    cache = {}

    def spurious(i):
        if i not in cache:
            cache[i] = not srcsets.genuine(fp.file_key(S[i]), keys[othi[i]])
        return cache[i]

    out = {"tag": tag, "metric": M, "margin": margin, "against": against, "min_informative": MIN_INF}
    for t in (0.5, 0.6, 0.7, 0.8):
        acc = (top >= t) & ((top - sec) >= margin) & gate
        rows = {}
        groups = {}
        for c in CFGS:
            if c != cfg_index:
                groups["cfg:" + c] = (st == "lab") & (cfg == c) & (mode != "some")
        groups["tree-shaken(some),all cfgs"] = (st == "lab") & (cfg != cfg_index) & (mode == "some")
        for lo, hi in BUCKETS:
            groups[f"inf:{lo}-{hi}"] = (st == "lab") & (cfg != cfg_index) & (mode != "some") & (qs >= lo) & (qs < hi)
        groups["ALL cross-config full modules"] = (st == "lab") & (cfg != cfg_index) & (mode != "some")
        for g, m in groups.items():
            a = m & acc
            ow = [i for i in np.nonzero(m & gate & (oth >= t))[0]]
            rows[g] = {
                "n": int(m.sum()),
                "accepted": int(a.sum()),
                "prec_pkg": round(float(pkg_ok[a].mean()), 4) if a.any() else None,
                "prec_file": round(float(file_ok[a].mean()), 4) if a.any() else None,
                "exact_version": round(float(ver_ok[a].mean()), 4) if a.any() else None,
                "recall_file": round(float((a & file_ok).sum() / max((m & in_index).sum(), 1)), 4),
                "ow_spurious": round(sum(spurious(i) for i in ow) / max(m.sum(), 1), 5),
                "ow_spurious_n": int(sum(spurious(i) for i in ow)),
            }
        negs = {g: int(((st == g) & acc).sum()) for g in ("cc-neg", "cc-negfn", "lab-app")}
        negs["of"] = {g: int((st == g).sum()) for g in ("cc-neg", "cc-negfn", "lab-app")}
        out[str(t)] = {"groups": rows, "app_negatives_accepted": negs}
        print(f"\n== {tag} {M} t={t} margin={margin} vs-other-{against} gate={MIN_INF}")
        for g, r in rows.items():
            print(f"  {g:34s} n={r['n']:6d} acc={r['accepted']:6d} prec_pkg={r['prec_pkg']} prec_file={r['prec_file']} "
                  f"exactver={r['exact_version']} recall={r['recall_file']:.3f} ow_spur={r['ow_spurious']:.4f} ({r['ow_spurious_n']})")
        print("  app negatives accepted:", negs)
    json.dump(out, open(f"{fp.HERE}/results/tables_{tag}_{M}_{against}.json", "w"), indent=1)


if __name__ == "__main__":
    main()
