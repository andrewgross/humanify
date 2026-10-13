"""Step B without boundaries: slide a window over a WHOLE bundle and ask the index what each window is.

  PYTHONHASHSEED=0 .venv/bin/python scan.py <stream> <n> <index-cfg> <W> <target> [maxpkgdf]
  target = combos      the 6 combo apps x 5 minified configs (per-token ground truth from the lab labels)
         = cc          the two Claude Code raw bundles (truth: CC positives' and negatives' byte ranges)

Each window = the shingles starting at W consecutive tokens (stride W/2). For each window: the index entry
that contains the largest share of the window's shingles (CQ = |W^E|/|W|), and Jaccard top. For combos also
the best entry from ANOTHER package than the window's true majority package (leave-package-out pressure).
Writes results/scan_<target>_<stream><n>_<cfg>_W<W>.npz + .json (window metadata).
"""
import json
import os
import sys
from multiprocessing import Pool

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402
import lex  # noqa: E402
import sim  # noqa: E402

LAB = "/work/exp095-library-fingerprints/lab/"
CFGS = ["bun-1.3.14", "bun-1.4.0", "bun-1.4.3", "esbuild-0.28.2", "rollup-4.64.5"]
CC = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-{v}/binary-decompiled/src/entrypoints/index.js"


def window_sets(text, stream, n, W):
    st, _, pos = lex.streams(text, positions=True)
    tok = np.fromiter((hash(t) for t in st[stream]), dtype=np.int64, count=len(st[stream])).view(np.uint64)
    pos = np.array(pos[stream], dtype=np.int64)
    nw = max(len(tok) - n + 1, 0)
    h = np.zeros(nw, dtype=np.uint64)
    with np.errstate(over="ignore"):
        for k in range(n):
            h += tok[k:k + nw] * fp.MULT[k]
    h = fp.mix(h)
    stride = W // 2
    starts = list(range(0, max(nw - W, 0) + 1, stride))
    sets = [np.unique(h[s:s + W]) for s in starts]
    # each window covers tokens [s, s+W+n-1)
    spans = [(int(pos[s]), int(pos[min(s + W + n - 2, len(pos) - 1)])) for s in starts]
    return sets, spans, pos


def combo_job(args):
    stream, n, W, cfg, combo = args
    d = f"{LAB}builds/{cfg}/min/{combo}/main/"
    text = open(d + "out.js", encoding="utf-8").read()
    labels = [json.loads(line) for line in open(d + "labels.jsonl")]
    sets, wspans, pos = window_sets(text, stream, n, W)
    # per-char label id
    lab_of = np.full(len(text) + 1, -1, dtype=np.int32)
    names = []
    for r in labels:
        if r["pkg"].startswith("<"):
            name = r["pkg"]
        else:
            if r["bundler"] == "rollup" and (r["purity"] or 0) < 0.9:
                name = "<impure>"
            else:
                name = r["pkg"]
        if name not in names:
            names.append(name)
        lab_of[r["startChar"]:r["endChar"]] = names.index(name)
    meta = []
    for (a, b) in wspans:
        ids = lab_of[pos[(pos >= a) & (pos <= b)]]
        vals, cnt = np.unique(ids, return_counts=True)
        top = vals[cnt.argmax()]
        meta.append({"cfg": cfg, "combo": combo, "start": a, "end": b,
                     "major": names[top] if top >= 0 else "<none>", "purity": float(cnt.max() / cnt.sum()),
                     "appShare": float(sum(c for v, c in zip(vals, cnt) if v >= 0 and names[v] == "<app>") / cnt.sum())})
    return sets, meta


def cc_job(args):
    stream, n, W, v = args
    text = open(CC.format(v=v), encoding="utf-8").read()
    sets, wspans, pos = window_sets(text, stream, n, W)
    # code-point index -> utf-8 byte offset
    cps = np.frombuffer(text.encode("utf-32-le"), dtype=np.uint32)
    blen = np.where(cps < 0x80, 1, np.where(cps < 0x800, 2, np.where(cps < 0x10000, 3, 4)))
    cum = np.zeros(len(cps) + 1, dtype=np.int64)
    np.cumsum(blen, out=cum[1:])
    meta = [{"cc": v, "start": int(cum[a]), "end": int(cum[b + 1])} for (a, b) in wspans]
    return sets, meta


def main():
    stream, n, cfg, W, target = sys.argv[1], int(sys.argv[2]), sys.argv[3], int(sys.argv[4]), sys.argv[5]
    maxpkgdf = int(sys.argv[6]) if len(sys.argv) > 6 else 0
    G = sim.setup(stream, n, cfg, maxpkgdf)
    if target == "combos":
        jobs = [(stream, n, W, c, f"combo-{k}@0.0.0") for c in CFGS for k in range(1, 7)]
        fn = combo_job
    else:
        jobs = [(stream, n, W, v) for v in ("2.1.198", "2.1.216")]
        fn = cc_job
    with Pool(len(jobs)) as pool:
        res = pool.map(fn, jobs)
    sets = [s for r in res for s in r[0]]
    meta = [m for r in res for m in r[1]]
    ptr = np.zeros(len(sets) + 1, dtype=np.int64)
    np.cumsum([len(s) for s in sets], out=ptr[1:])
    G["ptr"], G["data"] = ptr, np.concatenate(sets)
    # windows have no truth inside sim.block's category logic: give them a pkg id for leave-package-out
    pk = {}
    for k in G["keys"]:
        pk.setdefault(k.split("\t")[0], len(pk))
    G["qpkg"] = np.array([pk.get(m.get("major", ""), -1) for m in meta])
    G["qfile"] = np.full(len(meta), -2)
    G["qsrc"] = np.full(len(meta), -3)
    nq = len(meta)
    blocks = [(lo, min(lo + 500, nq)) for lo in range(0, nq, 500)]
    out = {}
    with Pool(48) as pool:
        for lo, r in pool.imap_unordered(sim.block, blocks):
            for k, v in r.items():
                if k not in out:
                    out[k] = np.zeros(nq, dtype=v.dtype)
                out[k][lo:lo + len(v)] = v
    tag = f"{target}_{stream}{n}" + (f"d{maxpkgdf}" if maxpkgdf else "") + f"_{cfg}_W{W}"
    np.savez_compressed(f"{fp.HERE}/results/scan_{tag}.npz", **out)
    json.dump(meta, open(f"{fp.HERE}/results/scan_{tag}.json", "w"))
    print(tag, "windows", nq)


if __name__ == "__main__":
    main()
