"""Shared helpers: load spans, build per-span shingle sets (CSR of sorted unique uint64), build the index."""
import json
import os

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "data")
MULT = np.array([0x9E3779B97F4A7C15, 0xC2B2AE3D27D4EB4F, 0x165667B19E3779F9, 0xD6E8FEB86659FD93,
                 0xFF51AFD7ED558CCD, 0xC4CEB9FE1A85EC53, 0x27D4EB2F165667C5, 0x94D049BB133111EB,
                 0xBF58476D1CE4E5B9, 0x2545F4914F6CDD1D], dtype=np.uint64)


def mix(x):
    x = x.copy()
    with np.errstate(over="ignore"):
        x ^= x >> np.uint64(30)
        x *= np.uint64(0xBF58476D1CE4E5B9)
        x ^= x >> np.uint64(27)
        x *= np.uint64(0x94D049BB133111EB)
        x ^= x >> np.uint64(31)
    return x


def load_spans():
    return [json.loads(line) for line in open(f"{DATA}/spans.jsonl")]


def shingle_sets(stream, n, cache=True):
    """CSR (ptr, data): span i's sorted unique shingle hashes are data[ptr[i]:ptr[i+1]]."""
    fn = f"{DATA}/sets_{stream}{n}"
    if cache and os.path.exists(fn + "_ptr.npy"):
        return np.load(fn + "_ptr.npy"), np.load(fn + "_data.npy", mmap_mode="r")
    tok = np.load(f"{DATA}/tok_{stream}.npy").view(np.uint64)
    off = np.load(f"{DATA}/off_{stream}.npy")
    T = len(tok)
    lens = np.diff(off)
    sid = np.repeat(np.arange(len(lens)), lens)
    nw = max(T - n + 1, 0)
    h = np.zeros(nw, dtype=np.uint64)
    with np.errstate(over="ignore"):
        for k in range(n):
            h += tok[k:k + nw] * MULT[k]
    h = mix(h)
    starts = np.arange(nw)
    valid = starts + n <= off[1:][sid[:nw]]
    hs, ss = h[valid], sid[:nw][valid]
    # spans shorter than n: one shingle of the whole stream
    short = np.nonzero((lens < n) & (lens > 0))[0]
    if len(short):
        extra = np.zeros(len(short), dtype=np.uint64)
        with np.errstate(over="ignore"):
            for j, i in enumerate(short):
                seg = tok[off[i]:off[i + 1]]
                extra[j] = mix(np.array([(seg * MULT[:len(seg)]).sum()], dtype=np.uint64))[0]
        hs = np.concatenate([hs, extra])
        ss = np.concatenate([ss, short])
    order = np.lexsort((hs, ss))
    hs, ss = hs[order], ss[order]
    keep = np.ones(len(hs), dtype=bool)
    keep[1:] = (hs[1:] != hs[:-1]) | (ss[1:] != ss[:-1])
    hs, ss = hs[keep], ss[keep]
    counts = np.bincount(ss, minlength=len(lens))
    ptr = np.zeros(len(lens) + 1, dtype=np.int64)
    np.cumsum(counts, out=ptr[1:])
    if cache:
        np.save(fn + "_ptr.npy", ptr)
        np.save(fn + "_data.npy", hs)
    return ptr, hs


def srcsha():
    return json.load(open(f"{DATA}/srcsha.json"))


def file_key(r):
    return f"{r['pkg']}\t{r['ver']}\t{r['file']}"


def build_index(spans, ptr, data, cfg, modes=("dyn", "req", "imp", "main")):
    """One entry per (pkg, ver, file) from config `cfg`: the largest COMPLETE instance (parts of one build
    merged), never a tree-shaken `some` span. Returns (keys, sets) with sets a list of uint64 arrays."""
    groups = {}
    for i, r in enumerate(spans):
        if r["set"] != "lab" or r["cfg"] != cfg or r["mode"] not in modes:
            continue
        groups.setdefault((file_key(r), r["build"]), []).append(i)
    best = {}
    for (fk, build), idxs in groups.items():
        s = np.unique(np.concatenate([data[ptr[i]:ptr[i + 1]] for i in idxs]))
        if fk not in best or len(s) > len(best[fk][1]):
            best[fk] = (build, s)
    keys = sorted(best)
    return keys, [best[k][1] for k in keys]


def lit_sets():
    """humanify's literal feature: string values (3+ chars) and identifiers of 5+ chars, as a CSR of hashes."""
    fn = f"{DATA}/sets_LIT"
    if os.path.exists(fn + "_ptr.npy"):
        return np.load(fn + "_ptr.npy"), np.load(fn + "_data.npy", mmap_mode="r")
    import pickle
    lits = pickle.load(open(f"{DATA}/lits.pkl", "rb"))
    arrs = [np.unique(np.array([hash(x) for x in L], dtype=np.int64).view(np.uint64)) for L in lits]
    ptr = np.zeros(len(arrs) + 1, dtype=np.int64)
    np.cumsum([len(a) for a in arrs], out=ptr[1:])
    data = np.concatenate(arrs)
    np.save(fn + "_ptr.npy", ptr)
    np.save(fn + "_data.npy", data)
    return ptr, data
