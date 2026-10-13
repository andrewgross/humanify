"""Similarity of labelled module PAIRS by relation, for every feature variant.

  PYTHONHASHSEED=0 .venv/bin/python pairs.py

A unit is one file's code in one build (the parts of an eager file merged). Relations:
  bun13_vs_bun140  same app/mode/file, Bun 1.3.14 vs Bun 1.4.0
  bun140_vs_bun143 same, Bun 1.4.0 vs 1.4.3
  bun140_vs_esbuild, bun140_vs_rollup, esbuild_vs_rollup
  req_vs_imp       Bun 1.4.0, same file+version, reached by require() vs import * (ESM: lazy vs eager)
  full_vs_some     Bun 1.4.0, whole import vs tree-shaken import of 3 names (eager)
  ver_patch/minor/major  Bun 1.4.0 req/dyn, same pkg+file, other version, SOURCE CHANGED
  ver_unchanged    same, but the source file is byte-identical across the two versions
  samepkg_otherfile  Bun 1.4.0, same pkg+version, different file
  otherpkg         Bun 1.4.0, random pair of different packages
Writes results/pairs.npz (raw J / containment per relation x variant) and results/pairs.json (percentiles).
"""
import itertools
import json
import os
import random
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402

VARIANTS = [(s, n) for s in ("H", "B", "BX", "L", "LX") for n in (3, 5, 8)] + [("LIT", 0)]
MAXPAIRS = 4000


def semver(v):
    core = v.split("-")[0].split("+")[0]
    p = [int(x) if x.isdigit() else 0 for x in core.split(".")[:3]]
    return tuple(p + [0] * (3 - len(p)))


def vdist(a, b):
    a, b = semver(a), semver(b)
    if a[0] != b[0]:
        return "major"
    if a[1] != b[1]:
        return "minor"
    return "patch"


def main():
    rnd = random.Random(95)
    spans = fp.load_spans()
    src = fp.srcsha()
    units = {}
    for i, r in enumerate(spans):
        if r["set"] != "lab":
            continue
        units.setdefault((r["cfg"], r["app"], r["mode"], fp.file_key(r)), []).append(i)
    ntok = {u: sum(spans[i]["ntok"] for i in idxs) for u, idxs in units.items()}
    by = {}
    for (cfg, app, mode, fk) in units:
        by.setdefault((app, mode, fk), {})[cfg] = (cfg, app, mode, fk)
    rel = {k: [] for k in ["bun13_vs_bun140", "bun140_vs_bun143", "bun140_vs_esbuild", "bun140_vs_rollup",
                           "esbuild_vs_rollup", "req_vs_imp", "full_vs_some", "ver_patch", "ver_minor", "ver_major",
                           "ver_unchanged", "samepkg_otherfile", "otherpkg"]}
    cpairs = {"bun13_vs_bun140": ("bun-1.3.14", "bun-1.4.0"), "bun140_vs_bun143": ("bun-1.4.0", "bun-1.4.3"),
              "bun140_vs_esbuild": ("bun-1.4.0", "esbuild-0.28.2"), "bun140_vs_rollup": ("bun-1.4.0", "rollup-4.64.5"),
              "esbuild_vs_rollup": ("esbuild-0.28.2", "rollup-4.64.5")}
    for (app, mode, fk), d in by.items():
        if mode == "some":
            continue
        for name, (a, b) in cpairs.items():
            if a in d and b in d:
                rel[name].append((d[a], d[b]))
    b140 = {u for u in units if u[0] == "bun-1.4.0"}
    idx = {}
    for u in b140:
        idx.setdefault((u[1], u[3]), {})[u[2]] = u
    for (app, fk), d in idx.items():
        if "req" in d and "imp" in d:
            rel["req_vs_imp"].append((d["req"], d["imp"]))
        if "some" in d and "imp" in d:
            rel["full_vs_some"].append((d["imp"], d["some"]))
    # versions: one unit per (pkg, ver, file) from req/dyn
    pv = {}
    for u in sorted(b140):
        if u[2] in ("req", "dyn"):
            pkg, ver, f = u[3].split("\t")
            pv.setdefault((pkg, f), {}).setdefault(ver, u)
    for (pkg, f), vs in pv.items():
        for va, vb in itertools.combinations(sorted(vs), 2):
            ka, kb = f"{pkg}\t{va}\t{f}", f"{pkg}\t{vb}\t{f}"
            if src.get(ka) == src.get(kb):
                rel["ver_unchanged"].append((vs[va], vs[vb]))
            else:
                rel["ver_" + vdist(va, vb)].append((vs[va], vs[vb]))
    bypv = {}
    for u in b140:
        if u[2] in ("req", "dyn"):
            pkg, ver, f = u[3].split("\t")
            bypv.setdefault((pkg, ver), {}).setdefault(f, u)
    for (pkg, ver), fs in bypv.items():
        L = sorted(fs.values())
        for a, b in itertools.combinations(L, 2):
            if src.get(a[3]) != src.get(b[3]):
                rel["samepkg_otherfile"].append((a, b))
    allu = sorted(u for u in b140 if u[2] in ("req", "dyn"))
    while len(rel["otherpkg"]) < MAXPAIRS * 3:
        a, b = rnd.choice(allu), rnd.choice(allu)
        if a[3].split("\t")[0] != b[3].split("\t")[0]:
            rel["otherpkg"].append((a, b))
    for k in rel:
        rnd.shuffle(rel[k])
        rel[k] = rel[k][:MAXPAIRS]
        print(k, len(rel[k]), flush=True)
    raw = {}
    summary = {}
    for s, n in VARIANTS:
        ptr, data = fp.lit_sets() if s == "LIT" else fp.shingle_sets(s, n)
        cache = {}

        def uset(u):
            if u not in cache:
                cache[u] = np.unique(np.concatenate([data[ptr[i]:ptr[i + 1]] for i in units[u]]))
            return cache[u]

        for k, prs in rel.items():
            J = np.zeros(len(prs), np.float32)
            CA = np.zeros(len(prs), np.float32)
            CB = np.zeros(len(prs), np.float32)
            sz = np.zeros(len(prs), np.int32)
            for j, (a, b) in enumerate(prs):
                A, Bs = uset(a), uset(b)
                inter = len(np.intersect1d(A, Bs, assume_unique=True))
                uni = len(A) + len(Bs) - inter
                J[j] = inter / uni if uni else 0
                CA[j] = inter / len(A) if len(A) else 0
                CB[j] = inter / len(Bs) if len(Bs) else 0
                sz[j] = min(ntok[a], ntok[b])
            raw[f"{s}{n}|{k}|J"] = J
            raw[f"{s}{n}|{k}|CA"] = CA
            raw[f"{s}{n}|{k}|CB"] = CB
            raw[f"{s}{n}|{k}|size"] = sz
            summary[f"{s}{n}|{k}"] = {
                "n": len(prs),
                "J_pct_5_25_50_75_95": [round(float(x), 3) for x in np.percentile(J, [5, 25, 50, 75, 95])] if len(J) else None,
                "J_pct_ge50tok": [round(float(x), 3) for x in np.percentile(J[sz >= 50], [5, 25, 50, 75, 95])] if (sz >= 50).any() else None,
            }
        print(s, n, "done", flush=True)
    os.makedirs(f"{fp.HERE}/results", exist_ok=True)
    np.savez_compressed(f"{fp.HERE}/results/pairs.npz", **raw)
    json.dump(summary, open(f"{fp.HERE}/results/pairs.json", "w"), indent=1)


if __name__ == "__main__":
    main()
