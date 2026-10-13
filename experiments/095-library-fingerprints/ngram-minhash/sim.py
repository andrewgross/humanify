"""Every query span x every index entry: exact shingle intersections -> per-query summary statistics.

  PYTHONHASHSEED=0 .venv/bin/python sim.py <stream> <n> <index-cfg>
  e.g.  sim.py B 5 bun-1.4.0

Index: one entry per (pkg, ver, file) built from <index-cfg> (fp.build_index).
Queries: every span in data/spans.jsonl (all configs, all modes, CC sets, app spans).
Writes results/sim_<stream><n>_<cfg>.npz with, per query:
  qsize                     |Q| (unique shingles)
  for metric m in J (Jaccard), CQ (|Q^E|/|Q|, query contained in entry), CE (|Q^E|/|E|):
    m_same      best over entries with the SAME source file content (correct module)
    m_samefile  best over same pkg+file but different source (another version, changed)
    m_samepkg   best over same pkg, other file
    m_other     best over other packages (different source)       -> the false-positive pressure
    m_top, m_topi   best overall and its entry index
    m_second    best over entries whose source differs from the top entry's source (margin)
  other_i (J and CQ): argmax entry index of m_other
"""
import json
import os
import sys
from multiprocessing import Pool

import numpy as np
import scipy.sparse as sp

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402

COMMON_DF = 150
THRESH = (0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9)  # shingles in more index entries than this go through the dense product


def setup(stream, n, cfg, maxpkgdf=0, sample=1):
    global G
    spans = fp.load_spans()
    ptr, data = fp.lit_sets() if stream == "LIT" else fp.shingle_sets(stream, n)
    if sample > 1:
        # keep only the shingles whose hash is 0 mod `sample` (a fixed, content-defined subsample: both sides
        # keep the same shingles, so containment and Jaccard are estimated on 1/sample of the set)
        data = np.asarray(data)
        keep = (data % np.uint64(sample)) == 0
        ck = np.zeros(len(keep) + 1, dtype=np.int64)
        np.cumsum(keep, out=ck[1:])
        cnt = ck[ptr[1:]] - ck[ptr[:-1]]
        ptr = np.zeros(len(ptr), dtype=np.int64)
        np.cumsum(cnt, out=ptr[1:])
        data = data[keep]
    keys, isets = fp.build_index(spans, ptr, data, cfg)
    src = fp.srcsha()
    allv = np.concatenate(isets)
    vocab, inv = np.unique(allv, return_inverse=True)
    rows = np.repeat(np.arange(len(isets)), [len(s) for s in isets])
    E = sp.csr_matrix((np.ones(len(inv), dtype=np.float32), (rows, inv)), shape=(len(isets), len(vocab)))
    df = np.asarray(E.sum(axis=0)).ravel()
    # weights: 1 for shingles; humanify's literal IDF log((N+1)/(df+0.5)) for LIT (unseen literal = 8.0)
    w = np.log((len(isets) + 1) / (df + 0.5)).astype(np.float32) if stream == "LIT" else np.ones(len(vocab), np.float32)
    if maxpkgdf:
        # boilerplate filter: a shingle found in more than `maxpkgdf` PACKAGES identifies nothing; weight 0 on
        # both sides (it neither matches nor counts in any size)
        pkgs = np.array([pkg_id.setdefault(k.split("\t")[0], len(pkg_id)) for pkg_id in [{}] for k in keys])
        pairs = np.unique(np.stack([inv, pkgs[rows]]), axis=1)
        pkgdf = np.bincount(pairs[0], minlength=len(vocab))
        w[pkgdf > maxpkgdf] = 0
    E = E @ sp.diags(w)
    E.eliminate_zeros()
    common = np.nonzero(df > COMMON_DF)[0]
    rare = np.nonzero(df <= COMMON_DF)[0]
    Er = E[:, rare].T.tocsr()
    Ec = E[:, common].toarray()
    # (E carries the weights; the query side is binary, so inter = weighted intersection)
    esize = np.asarray(E.sum(axis=1)).ravel().astype(np.float32)
    # entry attributes as ints
    pk = {}
    fk = {}
    sk = {}
    epkg = np.array([pk.setdefault(k.split("\t")[0], len(pk)) for k in keys])
    efile = np.array([fk.setdefault(k.split("\t")[0] + "\t" + k.split("\t")[2], len(fk)) for k in keys])
    esrc = np.array([sk.setdefault(src.get(k, k), len(sk)) for k in keys])
    qpkg = np.array([pk.get(r["pkg"], -1) for r in spans])
    qfile = np.array([fk.get(f"{r['pkg']}\t{r['file']}", -2) for r in spans])
    qsrc = np.array([sk.get(src.get(fp.file_key(r), "?"), -3) if r["set"] == "lab" else -3 for r in spans])
    # queries restricted to index vocabulary (columns rare / common) + full sizes
    pos_r = -np.ones(len(vocab), dtype=np.int64)
    pos_r[rare] = np.arange(len(rare))
    pos_c = -np.ones(len(vocab), dtype=np.int64)
    pos_c[common] = np.arange(len(common))
    unseen = 8.0 if stream == "LIT" else 1.0
    G = dict(w=w, unseen=unseen, spans=spans, ptr=ptr, data=data, vocab=vocab, pos_r=pos_r, pos_c=pos_c, Er=Er, Ec=Ec, esize=esize,
             epkg=epkg, efile=efile, esrc=esrc, qpkg=qpkg, qfile=qfile, qsrc=qsrc, nrare=len(rare),
             ncommon=len(common), keys=keys)
    return G


def qmatrix(lo, hi):
    ptr, data, vocab = G["ptr"], G["data"], G["vocab"]
    seg = np.asarray(data[ptr[lo]:ptr[hi]])
    rows = np.repeat(np.arange(hi - lo), np.diff(ptr[lo:hi + 1]))
    loc = np.searchsorted(vocab, seg)
    loc[loc >= len(vocab)] = 0
    hit = vocab[loc] == seg
    # weighted query size: index-vocabulary items at their weight, the rest at the unseen weight
    qsize = np.full(hi - lo, 0.0)
    np.add.at(qsize, rows[hit], G["w"][loc[hit]])
    np.add.at(qsize, rows[~hit], G["unseen"])
    rows, loc = rows[hit], loc[hit]
    pr, pc = G["pos_r"][loc], G["pos_c"][loc]
    mr = pr >= 0
    Qr = sp.csr_matrix((np.ones(mr.sum(), dtype=np.float32), (rows[mr], pr[mr])), shape=(hi - lo, G["nrare"]))
    Qc = np.zeros((hi - lo, G["ncommon"]), dtype=np.float32)
    Qc[rows[~mr], pc[~mr]] = 1
    return Qr, Qc, qsize.astype(np.float32)


def block(args):
    lo, hi = args
    Qr, Qc, qsize = qmatrix(lo, hi)
    inter = (Qr @ G["Er"]).toarray() + Qc @ G["Ec"].T
    es = G["esize"][None, :]
    qs = qsize[:, None]
    res = {"qsize": qsize}
    same = G["esrc"][None, :] == G["qsrc"][lo:hi, None]
    spkg = G["epkg"][None, :] == G["qpkg"][lo:hi, None]
    sfile = G["efile"][None, :] == G["qfile"][lo:hi, None]
    cats = {"same": same, "samefile": sfile & ~same, "samepkg": spkg & ~sfile & ~same, "other": ~spkg & ~same}
    with np.errstate(divide="ignore", invalid="ignore"):
        mets = {"J": inter / (qs + es - inter), "CQ": inter / qs, "CE": inter / es}
    mets = {k: np.nan_to_num(v, nan=0.0, posinf=0.0) for k, v in mets.items()}
    for m, M in mets.items():
        for c, mask in cats.items():
            res[f"{m}_{c}"] = np.where(mask, M, -1).max(axis=1)
        topi = M.argmax(axis=1)
        res[f"{m}_top"] = M[np.arange(hi - lo), topi]
        res[f"{m}_topi"] = topi
        diff = G["esrc"][None, :] != G["esrc"][topi][:, None]
        res[f"{m}_second"] = np.where(diff, M, -1).max(axis=1)
        dfile = G["efile"][None, :] != G["efile"][topi][:, None]
        res[f"{m}_secondfile"] = np.where(dfile & diff, M, -1).max(axis=1)
        dpkg = G["epkg"][None, :] != G["epkg"][topi][:, None]
        res[f"{m}_secondpkg"] = np.where(dpkg & diff, M, -1).max(axis=1)
        if m in ("J", "CQ"):
            oth = np.where(cats["other"], M, -1)
            res[f"{m}_otheri"] = oth.argmax(axis=1)
            # how many OTHER-package entries reach each threshold (false-hit rate per index entry)
            for t in THRESH:
                res[f"{m}_nother{int(t * 100)}"] = (oth >= t).sum(axis=1).astype(np.int32)
    return lo, res


def main():
    stream, n, cfg = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    maxpkgdf = int(sys.argv[4]) if len(sys.argv) > 4 else 0
    sample = int(sys.argv[5]) if len(sys.argv) > 5 else 1
    setup(stream, n, cfg, maxpkgdf, sample)
    tag = f"{stream}{n}" + (f"d{maxpkgdf}" if maxpkgdf else "") + (f"s{sample}" if sample > 1 else "") + f"_{cfg}"
    nq = len(G["spans"])
    blocks = [(lo, min(lo + 500, nq)) for lo in range(0, nq, 500)]
    out = {}
    with Pool(48) as pool:
        for lo, res in pool.imap_unordered(block, blocks):
            for k, v in res.items():
                if k not in out:
                    out[k] = np.zeros(nq, dtype=v.dtype)
                out[k][lo:lo + len(v)] = v
    os.makedirs(f"{fp.HERE}/results", exist_ok=True)
    np.savez_compressed(f"{fp.HERE}/results/sim_{tag}.npz", **out)
    with open(f"{fp.HERE}/results/index_{tag}.json", "w") as f:
        json.dump({"keys": G["keys"], "esize": G["esize"].tolist(), "rare": G["nrare"], "common": G["ncommon"],
                   "vocab": int(len(G["vocab"]))}, f)
    print(tag, "entries", len(G["keys"]), "vocab", len(G["vocab"]), "common", G["ncommon"])


if __name__ == "__main__":
    main()
