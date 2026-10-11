"""n-gram Jaccard (B5) scores of every query at every scale and filter level.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/jc.py
For index scale R (entries with minpos <= R) and filter d (drop shingles present in more than d packages of THAT
index; d=0 = no filter): per query, qsize = informative shingles (present and kept) + shingles the index never saw,
J per entry = |Q & E| / (qsize + |E| - |Q & E|) over kept shingles; best entry, its package, and the best J of any
OTHER package (margin).  -> data/jc-<fam>-R<R>-d<d>.npz  (sid, qsize, J, Jpkg, J2)
"""
import json, sys
import numpy as np, scipy.sparse as sp
from lib import *
from evalq import Q

z = np.load(D + "b5.npz")
brow = {k: i for i, k in enumerate(json.loads(str(z["keys"])))}
bptr, bdata = z["ptr"], z["data"]
DS = [int(x) for x in (sys.argv[1] if len(sys.argv) > 1 else "0,2,5,10").split(",")]
RS = [int(x) for x in (sys.argv[2] if len(sys.argv) > 2 else ",".join(map(str, SCALES))).split(",")]

for fam in ("bun", "esbuild"):
    J = np.load(D + f"jidx-{fam}.npz")
    epkg, emin, eptr, edata = J["pkg"], J["minpos"], J["ptr"], J["data"]
    ne = len(epkg)
    erow = np.repeat(np.arange(ne), np.diff(eptr))
    qs = sorted({q["sha"] for q in Q() if q["fam"] == fam})
    qrows = [brow[s] for s in qs]
    qlens = np.array([bptr[r + 1] - bptr[r] for r in qrows])
    qrow = np.repeat(np.arange(len(qs)), qlens)
    qdata = np.concatenate([bdata[bptr[r]:bptr[r + 1]] for r in qrows])
    for R in RS:
        em = emin <= R
        sel = em[erow]
        ed, er = edata[sel], erow[sel]
        # package df per shingle in this index
        key = np.empty(len(ed), dtype=[("s", "u8"), ("p", "i4")])
        key["s"], key["p"] = ed, epkg[er]
        key = np.unique(key)
        voc, df = np.unique(key["s"], return_counts=True)
        del key
        for d in DS:
            keep = (df <= d) if d else np.ones(len(voc), bool)
            kvoc = voc[keep]
            col = np.searchsorted(kvoc, ed); col[col >= len(kvoc)] = 0
            hit = kvoc[col] == ed
            E = sp.csr_matrix((np.ones(hit.sum(), np.float32), (er[hit], col[hit])), shape=(ne, len(kvoc)))
            esize = np.asarray(E.sum(1)).ravel()
            # query side
            pos_all = np.searchsorted(voc, qdata); pos_all[pos_all >= len(voc)] = 0
            present = voc[pos_all] == qdata
            unseen = np.bincount(qrow[~present], minlength=len(qs))
            qc = np.searchsorted(kvoc, qdata); qc[qc >= len(kvoc)] = 0
            qhit = (kvoc[qc] == qdata) if len(kvoc) else np.zeros(len(qdata), bool)
            Qm = sp.csr_matrix((np.ones(qhit.sum(), np.float32), (qrow[qhit], qc[qhit])), shape=(len(qs), len(kvoc)))
            qsize = np.asarray(Qm.sum(1)).ravel() + unseen
            out = dict(qsize=qsize.astype(np.float32), J=np.zeros(len(qs), np.float32), Jpkg=np.full(len(qs), -1, np.int32), J2=np.zeros(len(qs), np.float32), Jent=np.full(len(qs), -1, np.int32))
            CH = 2000
            for a in range(0, len(qs), CH):
                inter = (Qm[a:a + CH] @ E.T).tocoo()
                if inter.nnz == 0:
                    continue
                qi, ei, iv = inter.row, inter.col, inter.data
                jv = iv / (qsize[a + qi] + esize[ei] - iv)
                jv[~em[ei]] = -1  # entries outside this scale
                # best per (query, package)
                pk = epkg[ei]
                o = np.lexsort((-jv, pk, qi))
                qi, pk, jv, ei = qi[o], pk[o], jv[o], ei[o]
                first = np.ones(len(qi), bool); first[1:] = (qi[1:] != qi[:-1]) | (pk[1:] != pk[:-1])
                qi, pk, jv, ei = qi[first], pk[first], jv[first], ei[first]
                o = np.lexsort((-jv, qi))
                qi, pk, jv, ei = qi[o], pk[o], jv[o], ei[o]
                rank = np.arange(len(qi)) - np.searchsorted(qi, qi, "left")
                b = rank == 0
                out["J"][a + qi[b]] = jv[b]; out["Jpkg"][a + qi[b]] = pk[b]; out["Jent"][a + qi[b]] = ei[b]
                s2 = rank == 1
                out["J2"][a + qi[s2]] = jv[s2]
            np.savez(D + f"jc-{fam}-R{R}-d{d}.npz", sha=np.array(qs), **out)
            print(fam, R, d, "entries", int(em.sum()), "kept vocab", len(kvoc), "of", len(voc), flush=True)
