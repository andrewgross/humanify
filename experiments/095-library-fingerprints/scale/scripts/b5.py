"""B5 shingle sets (ngram-minhash stream B: local names masked, literals by value; 5-token runs), one per task.
usage: PYTHONHASHSEED=0 python3 scripts/b5.py <tasks.jsonl> <out.npz> [nproc=24]
tasks: {k, path, slice?:[startChar,endChar]}  (the same task files features.mjs reads)
out: keys (json list), ptr (int64), data (uint64 sorted-unique per task) -- same hashing as ngram-minhash/fp.py
"""
import json, os, sys
from multiprocessing import Pool
import numpy as np
assert os.environ.get("PYTHONHASHSEED") == "0"
sys.path.insert(0, "/work/exp095-library-fingerprints/ngram-minhash")
from lex import lex_raw, _streams  # noqa
from fp import MULT, mix  # noqa
N = 5
_cache = [None, None]


def text_of(t):
    if t.get("slice"):
        if _cache[0] != t["path"]:
            _cache[0], _cache[1] = t["path"], open(t["path"], encoding="utf8").read()
        a, b = t["slice"]
        return _cache[1][a:b]
    return open(t["path"], encoding="utf8").read()


def shingles(text):
    toks = lex_raw(text)
    st, _ = _streams(toks)
    B = [t for _, t in st["B"]]
    if not B:
        return np.zeros(0, np.uint64)
    tok = np.fromiter((hash(t) for t in B), dtype=np.int64, count=len(B)).view(np.uint64)
    if len(tok) < N:
        with np.errstate(over="ignore"):
            return mix(np.array([(tok * MULT[:len(tok)]).sum()], dtype=np.uint64))
    nw = len(tok) - N + 1
    h = np.zeros(nw, dtype=np.uint64)
    with np.errstate(over="ignore"):
        for k in range(N):
            h += tok[k:k + nw] * MULT[k]
    return np.unique(mix(h))


def work(chunk):
    out = []
    for t in chunk:
        try:
            out.append((t["k"], shingles(text_of(t))))
        except Exception:
            out.append((t["k"], np.zeros(0, np.uint64)))
    return out


def main():
    inp, outp = sys.argv[1], sys.argv[2]
    nproc = int(sys.argv[3]) if len(sys.argv) > 3 else 24
    tasks = [json.loads(l) for l in open(inp) if l.strip()]
    tasks.sort(key=lambda t: t["path"])
    # contiguous chunks so slice tasks share a cached file
    C = max(1, min(400, len(tasks) // (nproc * 4) or 1))
    chunks = [tasks[i:i + C] for i in range(0, len(tasks), C)]
    keys, arrs = [], []
    with Pool(nproc) as p:
        for res in p.imap(work, chunks, chunksize=1):
            for k, a in res:
                keys.append(k)
                arrs.append(a)
    ptr = np.zeros(len(arrs) + 1, np.int64)
    np.cumsum([len(a) for a in arrs], out=ptr[1:])
    data = np.concatenate(arrs) if arrs else np.zeros(0, np.uint64)
    np.savez(outp, keys=np.array(json.dumps(keys)), ptr=ptr, data=data)
    print("b5", len(keys), "shingles", len(data))


if __name__ == "__main__":
    main()
