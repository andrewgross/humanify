"""MinHash signatures and LSH banding: how well does a compact signature stand in for the exact Jaccard?

  PYTHONHASHSEED=0 .venv/bin/python minhash.py <stream> <n> <index-cfg>

For a sample of cross-config lab queries (full modules) and every app negative: the exact Jaccard to
(a) the query's own file in the index and (b) its best wrong-package entry (from sim results), against the
MinHash estimate with k = 64, 128, 256 32-bit hashes. Then LSH banding (b bands x r rows): the share of
true pairs that become candidates, and the average number of candidates per query.
Writes results/minhash_<stream><n>_<cfg>.json.
"""
import json
import os
import random
import sys
from multiprocessing import Pool

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402

K = 256
SEEDS = fp.mix(np.arange(1, K + 1, dtype=np.uint64) * np.uint64(0x9E3779B97F4A7C15))
MASK32 = np.uint64(0xFFFFFFFF)


def signature(s):
    s = np.asarray(s, dtype=np.uint64)
    if not len(s):
        return np.full(K, 0xFFFFFFFF, dtype=np.uint32)
    out = np.empty(K, dtype=np.uint32)
    for lo in range(0, K, 32):
        h = fp.mix(s[:, None] ^ SEEDS[None, lo:lo + 32]) & MASK32
        out[lo:lo + 32] = h.min(axis=0)
    return out


def sigs(sets):
    with Pool(48) as pool:
        return np.stack(pool.map(signature, sets, chunksize=32))


def main():
    stream, n, cfg = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    tag = f"{stream}{n}_{cfg}"
    spans = fp.load_spans()
    ptr, data = fp.shingle_sets(stream, n)
    keys, isets = fp.build_index(spans, ptr, data, cfg)
    kpos = {k: i for i, k in enumerate(keys)}
    R = np.load(f"{fp.HERE}/results/sim_{tag}.npz")
    rnd = random.Random(7)
    lab = [i for i, r in enumerate(spans) if r["set"] == "lab" and r["cfg"] != cfg and r["mode"] != "some"
           and fp.file_key(r) in kpos and r["ntok"] >= 50]
    q = rnd.sample(lab, 20000)
    negs = [i for i, r in enumerate(spans) if r["set"] in ("cc-neg", "cc-negfn", "lab-app")]
    qs = q + negs
    qsets = [np.asarray(data[ptr[i]:ptr[i + 1]]) for i in qs]
    print("signatures", len(isets), len(qsets), flush=True)
    E = sigs(isets)
    Q = sigs(qsets)
    true_e = [kpos[fp.file_key(spans[i])] for i in q] + [-1] * len(negs)
    other_e = [int(R["J_otheri"][i]) for i in qs]
    top_e = [int(R["J_topi"][i]) for i in qs]

    def exact(a, b):
        inter = len(np.intersect1d(a, b, assume_unique=True))
        return inter / (len(a) + len(b) - inter)

    res = {"tag": tag, "queries": len(q), "negatives": len(negs), "entries": len(keys)}
    pairs = {"true": [(j, true_e[j]) for j in range(len(q))],
             "other": [(j, other_e[j]) for j in range(len(qs))],
             "neg_top": [(j, top_e[j]) for j in range(len(q), len(qs))]}
    for name, prs in pairs.items():
        ex = np.array([exact(qsets[j], isets[e]) for j, e in prs])
        row = {"n": len(prs), "exact_p50": round(float(np.median(ex)), 3)}
        for k in (64, 128, 256):
            est = np.array([(Q[j, :k] == E[e, :k]).mean() for j, e in prs])
            err = est - ex
            row[f"k{k}"] = {"abs_err_p50_p95_max": [round(float(x), 4) for x in np.percentile(np.abs(err), [50, 95, 100])]}
            for t in (0.5, 0.6, 0.7):
                row[f"k{k}"][f"flip@{t}"] = int(((ex >= t) != (est >= t)).sum())
                row[f"k{k}"][f"est>={t}_exact<{t - 0.1:.1f}"] = int(((est >= t) & (ex < t - 0.1)).sum())
        res[name] = row
    # LSH banding on k=128
    lsh = {}
    for k, b, r in ((128, 32, 4), (128, 16, 8), (128, 64, 2), (64, 16, 4), (256, 64, 4)):
        buckets = [dict() for _ in range(b)]
        for e in range(len(keys)):
            for bi in range(b):
                buckets[bi].setdefault(E[e, bi * r:(bi + 1) * r].tobytes(), []).append(e)
        found = np.zeros(len(q), bool)
        ncand = []
        for j in range(len(qs)):
            cand = set()
            for bi in range(b):
                cand.update(buckets[bi].get(Q[j, bi * r:(bi + 1) * r].tobytes(), ()))
            ncand.append(len(cand))
            if j < len(q) and true_e[j] in cand:
                found[j] = True
        if "ex_true" not in locals():
            ex_true = np.array([exact(qsets[j], isets[true_e[j]]) for j in range(len(q))])
        lsh[f"k{k}_b{b}_r{r}"] = {
            "s_curve_50pct_at_J": round((1 / b) ** (1 / r), 3),
            "true_pair_candidate_rate": round(float(found.mean()), 4),
            "candidate_rate_J>=0.5": round(float(found[ex_true >= 0.5].mean()), 4),
            "candidate_rate_J>=0.7": round(float(found[ex_true >= 0.7].mean()), 4),
            "mean_candidates_per_query": round(float(np.mean(ncand)), 1),
            "p99_candidates": int(np.percentile(ncand, 99)),
        }
    res["lsh"] = lsh
    res["bytes_per_entry"] = {f"k{k}": k * 4 for k in (64, 128, 256)}
    json.dump(res, open(f"{fp.HERE}/results/minhash_{tag}.json", "w"), indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
