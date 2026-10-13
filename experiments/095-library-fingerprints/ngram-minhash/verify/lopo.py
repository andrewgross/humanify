"""Adversarial re-measure of the open-world claim with an HONEST leave-one-package-out (LOPO) filter.

The original sim.py computes the boilerplate filter (drop shingles in > d packages) on the FULL index, i.e.
WITH the query's own package present, and then reads the best OTHER-package score. In a true open world the
own package is absent, so a shingle shared by the own package and exactly d other packages is NOT dropped.
Here, for each query package P, the filter is recomputed on the index minus P, and every query of P is scored
against the index minus P (and against the index minus P minus a random half of the other packages, for the
size trend).

  PYTHONHASHSEED=0 ../.venv/bin/python lopo.py <stream> <n> <d> [index_cfg] [pkg_fraction] [seed]
  -> results/lopo_<stream><n>d<d>_<cfg>_f<frac>_s<seed>.npz  (per query: qsize, best other J, CQ, argmax entry)
"""
import json, os, sys
from multiprocessing import Pool
import numpy as np, scipy.sparse as sp
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
import fp  # noqa

G = {}

def setup(stream, n, d, cfg, frac, seed):
    spans = fp.load_spans()
    ptr, data = fp.lit_sets() if stream == "LIT" else fp.shingle_sets(stream, n)
    keys, isets = fp.build_index(spans, ptr, data, cfg)
    allv = np.concatenate(isets)
    vocab, inv = np.unique(allv, return_inverse=True)
    rows = np.repeat(np.arange(len(isets)), [len(s) for s in isets])
    E = sp.csr_matrix((np.ones(len(inv), np.float32), (rows, inv)), shape=(len(isets), len(vocab)))
    E.sum_duplicates(); E.data[:] = 1
    pk = {}
    epkg = np.array([pk.setdefault(k.split("\t")[0], len(pk)) for k in keys])
    # package x vocab presence
    P = sp.csr_matrix((np.ones(len(inv), np.float32), (epkg[rows], inv)), shape=(len(pk), len(vocab)))
    P.sum_duplicates(); P.data[:] = 1
    pkgdf = np.asarray(P.sum(axis=0)).ravel()
    src = fp.srcsha()
    sk = {}
    esrc = np.array([sk.setdefault(src.get(k, k), len(sk)) for k in keys])
    # queries: cross-config full lab modules + CC/app sets
    qidx = [i for i, r in enumerate(spans) if (r["set"] == "lab" and r["cfg"] != cfg and r["mode"] != "some")
            or r["set"] != "lab"]
    qpkg = np.array([pk.get(spans[i]["pkg"], -1) for i in qidx])
    qsrc = np.array([sk.get(src.get(fp.file_key(spans[i]), "?"), -3) if spans[i]["set"] == "lab" else -3 for i in qidx])
    rng = np.random.default_rng(seed)
    G.update(spans=spans, ptr=ptr, data=data, vocab=vocab, E=E, P=P, pkgdf=pkgdf, epkg=epkg, esrc=esrc,
             qidx=np.array(qidx), qpkg=qpkg, qsrc=qsrc, d=d, npk=len(pk), frac=frac, seed=seed, keys=keys)

def qmat(ids):
    ptr, data, vocab = G["ptr"], G["data"], G["vocab"]
    rows, cols, unseen = [], [], np.zeros(len(ids))
    for j, i in enumerate(ids):
        seg = np.asarray(data[ptr[i]:ptr[i + 1]])
        loc = np.searchsorted(vocab, seg); loc[loc >= len(vocab)] = 0
        hit = vocab[loc] == seg
        rows.append(np.full(hit.sum(), j)); cols.append(loc[hit]); unseen[j] = (~hit).sum()
    r = np.concatenate(rows) if rows else np.zeros(0, int); c = np.concatenate(cols) if cols else np.zeros(0, int)
    return sp.csr_matrix((np.ones(len(r), np.float32), (r, c)), shape=(len(ids), len(vocab))), unseen

def work(pid):
    """pid = query package id (or -1 for queries whose package is not in the index)."""
    sel = np.nonzero(G["qpkg"] == pid)[0]
    if not len(sel):
        return pid, None
    rng = np.random.default_rng(G["seed"] * 100003 + pid + 7)
    keep_pk = np.ones(G["npk"], bool)
    if pid >= 0:
        keep_pk[pid] = False
    if G["frac"] < 1:
        others = np.nonzero(keep_pk)[0]
        drop = rng.choice(others, size=int(round(len(others) * (1 - G["frac"]))), replace=False)
        keep_pk[drop] = False
    # filter recomputed on the remaining packages only
    pdf = np.asarray(G["P"][np.nonzero(keep_pk)[0]].sum(axis=0)).ravel()
    w = (pdf <= G["d"]).astype(np.float32) if G["d"] else np.ones(len(pdf), np.float32)
    # shingles absent from the remaining index are "unseen" (weight 1 in the query size, never match)
    present = pdf > 0
    ekeep = keep_pk[G["epkg"]]
    Ek = G["E"][np.nonzero(ekeep)[0]] @ sp.diags(w)
    esize = np.asarray(Ek.sum(axis=1)).ravel()
    Q, unseen = qmat(G["qidx"][sel])
    qsize = np.asarray(Q @ np.where(present, w, 1.0)).ravel() + unseen
    inter = (Q @ Ek.T).toarray()
    # never count entries holding the query's exact source (a byte-identical vendored copy is not an error)
    esrc_k = G["esrc"][ekeep]
    same = esrc_k[None, :] == G["qsrc"][sel][:, None]
    with np.errstate(divide="ignore", invalid="ignore"):
        J = np.nan_to_num(inter / (qsize[:, None] + esize[None, :] - inter))
        CQ = np.nan_to_num(inter / qsize[:, None])
    J[same] = -1; CQ[same] = -1
    eidx = np.nonzero(ekeep)[0]
    ji, ci = J.argmax(1), CQ.argmax(1)
    r = np.arange(len(sel))
    # second-best entry from a different package than the top (margin), J only
    tp = G["epkg"][eidx][ji]
    J2 = np.where(G["epkg"][eidx][None, :] != tp[:, None], J, -1).max(1)
    return pid, dict(sel=sel, qsize=qsize.astype(np.float32), J=J[r, ji].astype(np.float32), Ji=eidx[ji],
                     J2=J2.astype(np.float32), CQ=CQ[r, ci].astype(np.float32), CQi=eidx[ci])

def main():
    stream, n, d = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    cfg = sys.argv[4] if len(sys.argv) > 4 else "bun-1.4.0"
    frac = float(sys.argv[5]) if len(sys.argv) > 5 else 1.0
    seed = int(sys.argv[6]) if len(sys.argv) > 6 else 0
    setup(stream, n, d, cfg, frac, seed)
    nq = len(G["qidx"])
    out = {k: np.zeros(nq, np.float32) for k in ("qsize", "J", "J2", "CQ")}
    out.update(Ji=np.zeros(nq, np.int64), CQi=np.zeros(nq, np.int64))
    with Pool(int(os.environ.get("NPROC", "24"))) as pool:
        for pid, res in pool.imap_unordered(work, list(range(-1, G["npk"]))):
            if res is None:
                continue
            for k in out:
                out[k][res["sel"]] = res[k]
    os.makedirs(f"{HERE}/results", exist_ok=True)
    tag = f"{stream}{n}d{d}_{cfg}_f{frac}_s{seed}"
    np.savez_compressed(f"{HERE}/results/lopo_{tag}.npz", qidx=G["qidx"], **out)
    json.dump(G["keys"], open(f"{HERE}/results/lopo_keys_{stream}{n}_{cfg}.json", "w"))
    print("done", tag, nq)

if __name__ == "__main__":
    main()
