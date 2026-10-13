"""Summarise scan.py results.

  PYTHONHASHSEED=0 .venv/bin/python scan_eval.py <tag>     e.g. combos_B5_bun-1.4.0_W128 / cc_B5_bun-1.4.0_W128

combos: a window is APP when >= 90% of its tokens are app code, LIB(p) when >= 90% are package p.
  app FP@t        app windows whose best containment CQ >= t
  lib recall@t    LIB windows with CQ >= t and the best entry from package p
  lib wrong@t     LIB windows with CQ >= t but another package on top
  leave-out FP@t  LIB windows whose best OTHER-package containment >= t (p not in the index)
cc: windows inside the 100 negative groups (app code) -> FP; windows inside positive factories -> recall;
  every window >= t -> which packages, how many bytes, inside CJS factories or in the lazy-ESM region.
"""
import json
import os
import sys
from collections import Counter, defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402

LAB = "/work/exp095-library-fingerprints/lab/"
TS = [0.5, 0.6, 0.7, 0.8, 0.9]
MIN_INF = int(os.environ.get("MIN_INF", "20"))  # a window must hold >= this many informative shingles


def union_bytes(ranges):
    tot = 0
    end = -1
    for a, b in sorted(ranges):
        if b <= end:
            continue
        tot += b - max(a, end)
        end = b
    return tot


def combos(tag, R, meta, keys):
    epkg = [k.split("\t")[0] for k in keys]
    cq, topi, oth = R["CQ_top"], R["CQ_topi"], R["CQ_other"]
    out = {}
    cfgs = sorted({m["cfg"] for m in meta})
    for cfg in cfgs + ["all"]:
        sel = [i for i, m in enumerate(meta) if cfg == "all" or m["cfg"] == cfg]
        app = [i for i in sel if meta[i]["appShare"] >= 0.9]
        lib = [i for i in sel if meta[i]["purity"] >= 0.9 and not meta[i]["major"].startswith("<")]
        row = {"windows": len(sel), "app": len(app), "lib": len(lib)}
        for t in TS:
            row[str(t)] = {
                "app_fp": int(sum(cq[i] >= t for i in app)),
                "lib_recall": round(sum(cq[i] >= t and epkg[topi[i]] == meta[i]["major"] for i in lib) / max(len(lib), 1), 4),
                "lib_wrong": int(sum(cq[i] >= t and epkg[topi[i]] != meta[i]["major"] for i in lib)),
                "leaveout_fp": round(sum(oth[i] >= t for i in lib) / max(len(lib), 1), 4),
            }
        out[cfg] = row
    # examples of app FPs and wrong-package windows at 0.7
    ex = []
    for i, m in enumerate(meta):
        if (m["appShare"] >= 0.9 and cq[i] >= 0.7) or (m["purity"] >= 0.9 and not m["major"].startswith("<")
                                                       and cq[i] >= 0.7 and epkg[topi[i]] != m["major"]):
            ex.append({**m, "CQ": round(float(cq[i]), 3), "top": keys[topi[i]]})
    out["examples@0.7"] = ex[:40]
    out["examples@0.7_count"] = len(ex)
    return out


def cc(tag, R, meta, keys):
    epkg = [k.split("\t")[0] for k in keys]
    cq, topi = R["CQ_top"], R["CQ_topi"]
    out = {}
    for v in ("2.1.198", "2.1.216"):
        idx = [i for i, m in enumerate(meta) if m["cc"] == v]
        negs = [json.loads(line) for line in open(f"{LAB}realworld/cc-{v}/negatives.jsonl")]
        poss = [json.loads(line) for line in open(f"{LAB}realworld/cc-{v}/positives.jsonl")]
        lay = json.load(open(f"{LAB}realworld/layout-{v}.json"))["modules"]
        cjs = sorted((m["start"], m["end"]) for m in lay if m["kind"] == "cjs")
        cjs_starts = np.array([a for a, b in cjs])
        cjs_ends = np.array([b for a, b in cjs])

        def inside(a, b, ranges):
            return any(x <= a and b <= y for x, y in ranges)

        neg_r = [(n["start"], n["end"]) for n in negs]
        pos_r = {(p["start"], p["end"]): p["pkg"] for p in poss}
        neg_w = [i for i in idx if inside(meta[i]["start"], meta[i]["end"], neg_r)]
        pos_w = [(i, pos_r[r]) for i in idx for r in pos_r if r[0] <= meta[i]["start"] and meta[i]["end"] <= r[1]]
        row = {"windows": len(idx), "neg_windows": len(neg_w), "pos_windows": len(pos_w)}
        for t in TS:
            hits = [i for i in idx if cq[i] >= t]
            in_cjs = []
            for i in hits:
                k = np.searchsorted(cjs_starts, meta[i]["start"], side="right") - 1
                in_cjs.append(k >= 0 and meta[i]["end"] <= cjs_ends[k])
            by_pkg = defaultdict(list)
            for i, c in zip(hits, in_cjs):
                by_pkg[(epkg[topi[i]], "cjs" if c else "esm-region")].append((meta[i]["start"], meta[i]["end"]))
            row[str(t)] = {
                "neg_fp": int(sum(cq[i] >= t for i in neg_w)),
                "pos_recall_pkg": round(sum(cq[i] >= t and epkg[topi[i]] == p for i, p in pos_w) / max(len(pos_w), 1), 4),
                "pos_wrong_pkg": int(sum(cq[i] >= t and epkg[topi[i]] != p for i, p in pos_w)),
                "hit_bytes_cjs": union_bytes([r for (p, w), rs in by_pkg.items() if w == "cjs" for r in rs]),
                "hit_bytes_esm_region": union_bytes([r for (p, w), rs in by_pkg.items() if w != "cjs" for r in rs]),
                "top_packages_esm_region": Counter({p: union_bytes(rs) for (p, w), rs in by_pkg.items() if w != "cjs"}).most_common(25),
                "top_packages_cjs": Counter({p: union_bytes(rs) for (p, w), rs in by_pkg.items() if w == "cjs"}).most_common(15),
            }
        out[v] = row
    return out


def main():
    tag = sys.argv[1]
    R = dict(np.load(f"{fp.HERE}/results/scan_{tag}.npz"))
    gate = R["qsize"] >= MIN_INF
    for k in ("CQ_top", "CQ_other"):
        R[k] = np.where(gate, R[k], -1)
    meta = json.load(open(f"{fp.HERE}/results/scan_{tag}.json"))
    itag = tag.split("_", 1)[1].rsplit("_W", 1)[0]
    keys = json.load(open(f"{fp.HERE}/results/index_{itag}.json"))["keys"]
    res = combos(tag, R, meta, keys) if tag.startswith("combos") else cc(tag, R, meta, keys)
    json.dump(res, open(f"{fp.HERE}/results/scaneval_{tag}_g{MIN_INF}.json", "w"), indent=1)
    print(json.dumps({k: v for k, v in res.items() if not k.startswith("examples")}, indent=1)[:6000])


if __name__ == "__main__":
    main()
