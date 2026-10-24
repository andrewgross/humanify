"""exp145: where the quoted remin2-terser judge dP falls inside each DEV 36-app subsample distribution.

usage: python3 pctl145.py <bootstrap.json>   (adds `judgePercentile` per set; draws recomputed, same seed)"""
import json
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from all145 import CANDS, K, N, dp, rows  # noqa: E402


def main(path):
    res = json.load(open(path))
    for name, (jdp, _, corp) in CANDS.items():
        if jdp is None:
            continue
        pooled = []
        for corpus, (results, base, cand) in corp.items():
            rs = rows(results, base, cand, f"{corpus}-terser")
            pooled += rs
            for tag, r in ((f"{corpus}-terser", rs),) + ((("pooled-terser", pooled),) if corpus == "remin" else ()):
                rng = random.Random(145)
                rep = len(r) <= K
                ds = [dp([rng.choice(r) for _ in range(K)] if rep else rng.sample(r, K)) for _ in range(N)]
                share = sum(1 for d in ds if d <= jdp / 100) / N
                tgt = res[name]["pooled-terser"] if tag == "pooled-terser" else res[name]["sets"][tag]
                tgt["judgePercentile"] = share
                print(f"{name:<36}{tag:<15} judge dP {jdp:+.3f}: share of dev 36-app draws at or below it {share:.4f}")
    json.dump(res, open(path, "w"), indent=1, sort_keys=True)


if __name__ == "__main__":
    main(sys.argv[1])
