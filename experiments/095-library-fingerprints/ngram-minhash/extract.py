"""Lex every span we study into the five token streams (hashed) + humanify literal sets.

Outputs data/spans.jsonl (one row per span, in array order) and data/tok_<S>.npy / data/off_<S>.npy
(concatenated int64 token hashes; span i = tok[off[i]:off[i+1]]) and data/lits.pkl.

Spans: every MINIFIED library span of the lab (rollup: purity >= 0.9), every minified <app> span,
the Claude Code positives, negatives and negative functions (2.1.198, 2.1.216).
Run:  PYTHONHASHSEED=0 .venv/bin/python extract.py
"""
import json
import os
import pickle
import sys
from multiprocessing import Pool

import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from lex import STREAMS, streams  # noqa: E402

LAB = "/work/exp095-library-fingerprints/lab/"
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data")


def rows():
    for line in open(LAB + "labels/all.jsonl"):
        r = json.loads(line)
        if not r["minified"]:
            continue
        if r["pkg"] in ("<runtime>", "<unmapped>", "<bundler-helper>"):
            continue
        if r["bundler"] == "rollup" and (r["purity"] is None or r["purity"] < 0.9):
            continue
        cfg = r["build"].split("/")[0]
        yield {
            "set": "lab-app" if r["pkg"] == "<app>" else "lab",
            "cfg": cfg,
            "app": r["app"],
            "mode": r["mode"],
            "pkg": r["pkg"],
            "ver": r["pkgVersion"],
            "file": r["file"],
            "kind": r["kind"],
            "part": r["part"],
            "parts": r["parts"],
            "bytes": r["bytes"],
            "build": r["build"],
            "idx": r["idx"],
            "path": LAB + r["textPath"],
        }
    for v in ("2.1.198", "2.1.216"):
        base = f"{LAB}realworld/cc-{v}/"
        for name, setname in (("positives", "cc-pos"), ("negatives", "cc-neg"), ("negative-functions", "cc-negfn")):
            for line in open(base + name + ".jsonl"):
                r = json.loads(line)
                yield {
                    "set": setname,
                    "cfg": "cc-" + v,
                    "app": "claude-code",
                    "mode": None,
                    "pkg": r.get("pkg", "<app>") if setname == "cc-pos" else "<app>",
                    "ver": r.get("pkgVersion"),
                    "file": r.get("member") or r.get("vendorFile") or r.get("treeFile"),
                    "kind": r.get("kind", "fn"),
                    "part": 0,
                    "parts": 1,
                    "bytes": r["bytes"] if "bytes" in r else r["end"] - r["start"],
                    "build": "cc-" + v,
                    "idx": None,
                    "path": LAB + r["textPath"],
                }


def work(row):
    text = open(row["path"], encoding="utf-8", errors="replace").read()
    st, lits = streams(text)
    return {s: np.fromiter((hash(t) for t in st[s]), dtype=np.int64, count=len(st[s])) for s in STREAMS}, sorted(lits)


def main():
    os.makedirs(OUT, exist_ok=True)
    allrows = list(rows())
    print("spans", len(allrows), flush=True)
    toks = {s: [] for s in STREAMS}
    lits = []
    with Pool(60) as pool:
        for k, (arrs, lit) in enumerate(pool.imap(work, allrows, chunksize=64)):
            for s in STREAMS:
                toks[s].append(arrs[s])
            lits.append(lit)
            if k % 20000 == 0:
                print(k, flush=True)
    for s in STREAMS:
        lens = np.array([len(a) for a in toks[s]], dtype=np.int64)
        off = np.zeros(len(lens) + 1, dtype=np.int64)
        np.cumsum(lens, out=off[1:])
        np.save(f"{OUT}/tok_{s}.npy", np.concatenate(toks[s]))
        np.save(f"{OUT}/off_{s}.npy", off)
        print(s, "tokens", off[-1], flush=True)
    with open(f"{OUT}/spans.jsonl", "w") as f:
        for r, t in zip(allrows, toks["H"]):
            r = dict(r)
            r["ntok"] = len(t)
            f.write(json.dumps(r) + "\n")
    with open(f"{OUT}/lits.pkl", "wb") as f:
        pickle.dump(lits, f)


if __name__ == "__main__":
    main()
