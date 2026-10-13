"""Source-level shingle sets (B stream, n=5, on the ORIGINAL installed file, unminified) for every labelled
(pkg, ver, file). Used to tell a GENUINE copy (two packages that ship the same code: chalk's vendored
ansi-styles, @smithy/core's inlined middleware-stack) from a SPURIOUS match. A wrong-package match counts as
genuine when the two SOURCE files share >= 50% of either one's shingles.

  PYTHONHASHSEED=0 .venv/bin/python srcsets.py   -> data/src_B5.pkl {key: uint64 array}
"""
import json
import os
import pickle
import sys
from multiprocessing import Pool

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402
import lex  # noqa: E402


def one(item):
    key, path = item
    st, _ = lex.streams(open(path, encoding="utf-8", errors="replace").read())
    tok = np.fromiter((hash(t) for t in st["B"]), dtype=np.int64, count=len(st["B"])).view(np.uint64)
    n = 5
    if len(tok) < n:
        with np.errstate(over="ignore"):
            return key, fp.mix(np.array([(tok * fp.MULT[:len(tok)]).sum()], dtype=np.uint64))
    nw = len(tok) - n + 1
    h = np.zeros(nw, dtype=np.uint64)
    with np.errstate(over="ignore"):
        for k in range(n):
            h += tok[k:k + nw] * fp.MULT[k]
    return key, np.unique(fp.mix(h))


def main():
    paths = json.load(open(f"{fp.DATA}/srcpath.json"))
    with Pool(60) as pool:
        out = dict(pool.map(one, sorted(paths.items()), chunksize=16))
    pickle.dump(out, open(f"{fp.DATA}/src_B5.pkl", "wb"))
    print(len(out))


_S = None


def genuine(ka, kb, thr=0.5):
    """True when the two source files share >= thr of either one's shingles (a real copy of code)."""
    global _S
    if _S is None:
        _S = pickle.load(open(f"{fp.DATA}/src_B5.pkl", "rb"))
    a, b = _S.get(ka), _S.get(kb)
    if a is None or b is None or not len(a) or not len(b):
        return False
    inter = len(np.intersect1d(a, b, assume_unique=True))
    return inter / min(len(a), len(b)) >= thr


if __name__ == "__main__":
    main()
