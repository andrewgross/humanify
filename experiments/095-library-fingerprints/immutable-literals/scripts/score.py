"""Score every query (lab minified spans, Claude Code sets) against the source-file index.

usage: tools/venv/bin/python scripts/score.py
writes results/scores-lab.jsonl, results/scores-cc.jsonl (one row per distinct query text)

Per query:
  tot      total weight (bits) of its literals; literals the index never saw count w_unknown each
  best*    best file overall (pkg, ver, file, matched bits)
  pkg2     best score of any OTHER package than the best one (the margin)
  truePkgS best score within the true package; otherS / otherPkg: best of all other packages
           (= what an index WITHOUT the true package would answer: the leave-package-out negative)
  sameOtherVerS  best within the true package but another version (leave-version-out)
  trueFileS      the labelled source file's own score (if in the index), cont = |Q in S| / |Q|
"""
import os, sys, json, collections
import numpy as np
sys.path.insert(0, "/work/exp095-library-fingerprints/immutable-literals/scripts")
from common import *
V = os.environ.get('VARIANT', '')  # weight variant suffix ('' = plain IDF, '-capvocab' = word-like literals capped)

vocab = Vocab()
src = load_sources(vocab)
idx = Index(src, vocab)
print("index files", len(idx.src), "packages", idx.N, "vocab", len(vocab.words))
key_of = {(r["pkg"], r["ver"], r["file"]): i for i, r in enumerate(idx.src)}
fver = np.array([r["ver"] for r in idx.src])
ffile = np.array([r["file"] for r in idx.src])
pkg_end = np.append(idx.pkg_start[1:], len(idx.src))
flen = np.array([len(r["ids"]) for r in idx.src])


def run(queries, out_path):
    """queries: list of (meta dict, lit list, truth (pkg, ver, file) or None)"""
    out = open(out_path, "w")
    CH = 1500
    for c0 in range(0, len(queries), CH):
        chunk = queries[c0 : c0 + CH]
        idsets = [to_ids(q[1], vocab) for q in chunk]
        Q, tot = idx.query_matrix(idsets, [q[1] for q in chunk])
        M = (Q @ idx.F.T).toarray()
        PM = np.maximum.reduceat(M, idx.pkg_start, axis=1)
        for qi, (meta, lits, truth) in enumerate(chunk):
            row, prow = M[qi], PM[qi]
            s = idsets[qi]
            r = dict(meta)
            r["n"] = len(s)
            r["unk"] = sum(1 for w in lits if vocab.lookup(w) is None or idx.df_pkg[vocab.lookup(w)] == 0)
            r["n"] = len(lits)
            r["tot"] = round(float(tot[qi]), 3)
            order = np.argsort(-prow)[:5]
            r["top"] = [[idx.pkgs[int(o)], round(float(prow[o]), 3)] for o in order]
            # per top package: literal count of its best-matching file (most specific origin = fewest literals)
            tf = []
            for o in order:
                a0, b0 = idx.pkg_start[int(o)], pkg_end[int(o)]
                sg = row[a0:b0]
                hit = np.nonzero(sg >= sg.max() - 1e-4)[0] + a0
                tf.append(int(min(flen[h] for h in hit)))
            r["topLen"] = tf
            bp = int(order[0])
            r["bestPkg"] = idx.pkgs[bp]
            r["bestS"] = round(float(prow[bp]), 3)
            r["pkg2"] = idx.pkgs[int(order[1])]
            r["pkg2S"] = round(float(prow[order[1]]), 3)
            a, b = idx.pkg_start[bp], pkg_end[bp]
            seg = row[a:b]
            mx = seg.max()
            ties = np.nonzero(seg >= mx - 1e-4)[0] + a
            r["bestVers"] = sorted(set(fver[ties].tolist()))[:20]
            r["bestFiles"] = sorted(set(ffile[ties].tolist()))[:5]
            if truth:
                tp, tv, tf = truth
                pi = idx.pkg_ix.get(tp)
                if pi is not None:
                    r["truePkgS"] = round(float(prow[pi]), 3)
                    pr = prow.copy(); pr[pi] = -1
                    op = int(np.argmax(pr))
                    r["otherPkg"] = idx.pkgs[op]
                    r["otherS"] = round(float(pr[op]), 3)
                    a3, b3 = idx.pkg_start[op], pkg_end[op]
                    j3 = int(np.argmax(row[a3:b3])) + a3
                    r["otherFile"] = f"{fver[j3]}/{ffile[j3]}"
                    a2, b2 = idx.pkg_start[pi], pkg_end[pi]
                    seg2 = row[a2:b2]
                    other = fver[a2:b2] != tv
                    r["sameOtherVerS"] = round(float(seg2[other].max()), 3) if other.any() else None
                    # which other-version file wins
                    if other.any():
                        j = int(np.argmax(np.where(other, seg2, -1))) + a2
                        r["sameOtherVerFile"] = ffile[j]
                ti = key_of.get((tp, tv, tf))
                if ti is not None:
                    S = set(idx.src[ti]["ids"].tolist())
                    inter = [i for i in s if i in S]
                    r["trueFileS"] = round(float(row[ti]), 3)
                    r["cont"] = round(len(inter) / max(len(s), 1), 4)
                    r["srcN"] = len(S)
                    r["srcCover"] = round(len(inter) / max(len(S), 1), 4)
            out.write(json.dumps(r) + "\n")
        print(out_path, c0 + len(chunk), "/", len(queries), flush=True)
    out.close()


def main():
    # lab: one query per distinct minified span text
    lits_by_sha = {}
    for l in open(DATA + "spans.jsonl"):
        r = json.loads(l)
        lits_by_sha[r["sha"]] = r["lits"]
    by_sha = collections.defaultdict(list)
    for l in open(LAB + "labels/all.jsonl"):
        r = json.loads(l)
        if not r["minified"]:
            continue
        by_sha[r["sha256"]].append(r)
    queries = []
    for sha, rows in by_sha.items():
        r0 = rows[0]
        cfgs = sorted({f'{x["bundler"]}-{x["bundlerVersion"]}' for x in rows})
        meta = {"sha": sha, "pkg": r0["pkg"], "ver": r0["pkgVersion"], "file": r0["file"], "kind": r0["kind"], "bytes": r0["bytes"],
                "cfgs": cfgs, "copies": len(rows), "minPurity": min((x["purity"] if x["purity"] is not None else 1) for x in rows),
                "modes": sorted({x["mode"] for x in rows}), "parts": r0["parts"]}
        truth = None if r0["pkg"].startswith("<") else (r0["pkg"], r0["pkgVersion"], r0["file"])
        queries.append((meta, lits_by_sha[sha], truth))
    run(queries, HERE + "results/scores-lab"+V+".jsonl")

    cq = []
    for l in open(DATA + "cc.jsonl"):
        r = json.loads(l)
        lits = r.pop("lits")
        cq.append((r, lits, None))
    run(cq, HERE + "results/scores-cc"+V+".jsonl")


if __name__ == '__main__':
    main()
