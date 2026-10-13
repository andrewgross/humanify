"""Literal containment (immutable-literals method) at every scale, index built from npm SOURCE.
usage: .venv/bin/python scripts/lt.py [--sponge]       (default: sponge files DROPPED, as SYNTHESIS §4 recommends)
Index at scale R: every source file of every package@version that a root with position <= R installs, package not
held out; pre-bundled / minified / CDN / browser dist files ("sponges", immutable-literals/verify/score_verify.py
regex) dropped. One row per (package, file content); package-level IDF weights w = log2(N/df_pkg); identifier-like
S:/P: literals capped at 2 bits (capvocab); a literal no indexed package has weighs log2(N)+1 (capped too).
Per query (module span literal set, lits.mjs 'min' mode): tot = total weight; score(file) = weight of shared
literals; per package the best file. Kept: top 4 packages whose score could reach 0.5*tot - 10 (exact scores).
-> data/lt-<R>[-sponge].pkl : {sha: (n, tot, top=[(pkg, score, fileLiterals), ...], topSpecific=[...])}
Specific variant: only files with <= RHO x the query's literal count (+20) compete (sponge-proof; derived at scale).
"""
import json, math, os, pickle, re, sys, collections
import numpy as np, scipy.sparse as sp
from lib import *
from evalq import Q

SPONGE_OK = "--sponge" in sys.argv
STOP = "--nostop" not in sys.argv  # stop-lists: Stainless SDK template files and es-shims template files leave the index
STAIN_FILE = re.compile(r"(^|/)(internal|core|_shims|_vendor)/|(^|/)(uploads|streaming|error|resource|core|api-promise|pagination)\.(m?js|cjs)$")
SHIM_FILE = re.compile(r"(^|/)(implementation|polyfill|shim|auto)\.js$")
SP = re.compile(r"(^|/)[^/]*(bundled|bundle\.|\.min\.|\.global\.|^cdn\.|/cdn\.|\.umd\.|browser)[^/]*$")
WORDY = re.compile(r"^[SP]:[A-Za-z_$][A-Za-z0-9_$-]*$")
CAP = 2.0
RHO = 3.0  # specificity: a file may claim only if it holds <= RHO x the query's literals (+20)
CMIN, MARG = 0.5, 20.0  # exact for sweeps with cover >= 0.5 and margin <= 20

pos = json.load(open(D + "srcpos.json"))
vocab = {}
content = {}
for l in open(D + "srclits-content.jsonl", encoding="utf8", errors="surrogatepass"):
    try:
        r = json.loads(l)
    except Exception:
        continue
    content[r["csha"]] = np.array(sorted({vocab.setdefault(w, len(vocab)) for w in r["lits"]}), np.int64)
stain_csha = {c for c in content if False}
stain, files_of = set(), collections.defaultdict(set)
if STOP:
    for l in open(D + "srclits-content.jsonl", encoding="utf8", errors="surrogatepass"):
        if "X-Stainless-Lang" in l or "x-stainless-lang" in l:
            try:
                stain_csha.add(json.loads(l)["csha"])
            except Exception:
                pass
    for l in open(D + "srcfiles.jsonl"):
        r = json.loads(l)
        files_of[r["pkg"]].add(r["file"])
        if r["csha"] in stain_csha:
            stain.add(r["pkg"])
shimpk = {p for p, fs in files_of.items() if any(f.endswith("polyfill.js") for f in fs) and any(f.endswith("shim.js") for f in fs)}
files = {}  # (pkg, csha) -> minpos
dropped = stopped = 0
for l in open(D + "srcfiles.jsonl"):
    r = json.loads(l)
    if r["pkg"] in HELD:
        continue
    if STOP and ((r["pkg"] in stain and STAIN_FILE.search(r["file"])) or (r["pkg"] in shimpk and SHIM_FILE.search(r["file"]))):
        stopped += 1
        continue
    if not SPONGE_OK and SP.search(r["file"]):
        dropped += 1
        continue
    if r["csha"] not in content or not len(content[r["csha"]]):
        continue
    k = (r["pkg"], r["csha"])
    p = pos.get(r["pkg"] + "@" + r["ver"], 10**9)
    files[k] = min(files.get(k, 10**9), p)
print("index files", len(files), "sponge files dropped", dropped, "stop-listed files", stopped, "stainless", sorted(stain), "vocab", len(vocab), flush=True)
fk = sorted(files)
fpkgname = sorted({k[0] for k in fk})
fpid = {p: i for i, p in enumerate(fpkgname)}
fpkg = np.array([fpid[k[0]] for k in fk], np.int32)
fmin = np.array([files[k] for k in fk], np.int32)
rows = np.repeat(np.arange(len(fk)), [len(content[k[1]]) for k in fk])
cols = np.concatenate([content[k[1]] for k in fk])
V0 = len(vocab)
wordy = np.zeros(V0, bool)
for w, i in vocab.items():
    if WORDY.match(w):
        wordy[i] = True
flen = np.array([len(content[k[1]]) for k in fk])

qlit = qlits()
qs = sorted({q["sha"] for q in Q()})
qids = []
for s in qs:
    L = qlit.get(s, [])
    qids.append(np.array(sorted({vocab.setdefault(w, len(vocab)) for w in L}), np.int64))
V = len(vocab)
wordy = np.concatenate([wordy, np.zeros(V - V0, bool)])
for w, i in vocab.items():
    if i >= V0 and WORDY.match(w):
        wordy[i] = True

for R in SCALES:
    fm = fmin <= R
    F = sp.csr_matrix((np.ones(fm[rows].sum(), np.float32), (rows[fm[rows]], cols[fm[rows]])), shape=(len(fk), V))
    P = sp.csr_matrix((np.ones(fm[rows].sum(), np.float32), (fpkg[rows[fm[rows]]], cols[fm[rows]])), shape=(len(fpkgname), V))
    P.sum_duplicates(); P.data[:] = 1
    df = np.asarray(P.sum(0)).ravel()
    N = int((np.asarray(P.sum(1)).ravel() > 0).sum())
    with np.errstate(divide="ignore"):
        w = np.where(df > 0, np.log2(N / np.maximum(df, 1)), 0.0)
    w = np.where(wordy & (w > CAP), CAP, w).astype(np.float32)
    wunk = math.log2(N) + 1
    wunk_v = np.where(wordy, min(wunk, CAP), wunk).astype(np.float32)
    # query matrix (known literals weighted) + totals
    qr = np.repeat(np.arange(len(qs)), [len(a) for a in qids])
    qc = np.concatenate(qids) if qids else np.zeros(0, np.int64)
    known = df[qc] > 0
    tot = np.bincount(qr, weights=np.where(known, w[qc], wunk_v[qc]), minlength=len(qs))
    Qm = sp.csr_matrix((w[qc[known]], (qr[known], qc[known])), shape=(len(qs), V))
    out = {}
    CH = 4000
    FT = F.T.tocsr()
    for a in range(0, len(qs), CH):
        Qc = Qm[a:a + CH]
        UB = (Qc @ P.T).toarray()  # package upper bound (all literals of the package)
        tc = tot[a:a + CH]
        nq = np.array([len(qids[a + j]) for j in range(len(tc))])
        cand = (UB >= (CMIN * tc[:, None] - MARG)) & (UB >= 5) & (nq > 5)[:, None]
        qi, pi = np.nonzero(cand)
        res = collections.defaultdict(list)
        if len(qi):
            cp = np.unique(pi)
            frows = np.nonzero(np.isin(fpkg, cp) & fm)[0]
            S_ = (Qc @ F[frows].T).tocoo()
            # best file per (query, package), only candidate pairs
            fp = fpkg[frows[S_.col]]
            ok = cand[S_.row, fp]
            r0, f0, v0 = S_.row[ok], frows[S_.col[ok]], S_.data[ok]
            for variant, sel in (("all", np.ones(len(r0), bool)), ("spec", flen[f0] <= RHO * nq[r0] + 20)):
                r_, f_, v_ = r0[sel], f0[sel], v0[sel]
                p_ = fpkg[f_]
                o = np.lexsort((-v_, p_, r_))
                r_, p_, v_, f_ = r_[o], p_[o], v_[o], f_[o]
                first = np.ones(len(r_), bool); first[1:] = (r_[1:] != r_[:-1]) | (p_[1:] != p_[:-1])
                for r, p, v, f in zip(r_[first].tolist(), p_[first].tolist(), v_[first].tolist(), f_[first].tolist()):
                    res[(variant, r)].append((fpkgname[p], round(v, 3), int(flen[f])))
        for j in range(len(Qc.indptr) - 1):
            s = qs[a + j]
            top = sorted(res.get(("all", j), []), key=lambda x: -x[1])[:4]
            topS = sorted(res.get(("spec", j), []), key=lambda x: -x[1])[:4]
            out[s] = (len(qids[a + j]), round(float(tc[j]), 3), top, topS)
        print(R, a + len(Qc.indptr) - 1, "/", len(qs), flush=True)
    pickle.dump(out, open(D + f"lt-{R}{'-sponge' if SPONGE_OK else ''}{'' if STOP else '-nostop'}.pkl", "wb"))
    print("scale", R, "packages", N, "files", int(fm.sum()), flush=True)
