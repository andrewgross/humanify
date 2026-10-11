"""Index size per family and kind at every scale: what a shipped data file would hold, written and measured.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/index_size.py  -> data/shipped/R<R>/<file> and results/index-size.json
Kinds (each written as one compressed .npz; sizes are the real file sizes):
  wm      whole-module: unique (normM, normV, package) rows + per-row version list (uint16 ids)
  units   sub-tree units >= 30 nodes and >= 3 content tokens: unique (hash, package) rows (+ smallest module bytes)
  units20 the same at >= 20 nodes, no content floor (the original rule, for comparison)
  b5      n-gram entries: per (package, version, file) the B5 shingles that survive the d=2 filter (CSR)
  lits    literal index from npm source: per (package, file content) the literal ids (CSR) + the literal strings
Entries per KB = rows / KB of distinct minified library code in that family slice (largest instance per
(package, version, file)).
"""
import json, os, re, collections, gzip, io
import numpy as np
from lib import *

SH = D + "shipped/"
names = json.load(open(D + "pkgs.json"))
res = []


def fsize(p):
    return os.path.getsize(p)


# distinct minified library code per family and scale
code_kb = {}
for fam in ("bun", "esbuild"):
    best = {}
    for s in index_rows(fam):
        t = (s["pkg"], s["ver"], s["file"], s["part"])
        b = best.get(t)
        if b is None or s["bytes"] > b[0] or s["pos"] < b[1]:
            best[t] = (max(s["bytes"], b[0] if b else 0), min(s["pos"], b[1] if b else 10**9))
    for R in SCALES:
        code_kb[(fam, R)] = sum(v[0] for v in best.values() if v[1] <= R) / 1024
# version ids for wm rows
for R in SCALES:
    os.makedirs(SH + f"R{R}", exist_ok=True)
    row = dict(R=R, pkgs=len(indexed_pkgs(R)), pkgsBun=len(indexed_pkgs(R, "bun")), pkgsEsbuild=len(indexed_pkgs(R, "esbuild")),
               roots=sum(1 for r in ROOTS["roots"] if r["pos"] <= R))
    for fam in ("bun", "esbuild"):
        row[f"{fam}.codeMB"] = round(code_kb[(fam, R)] / 1024, 1)
        W = np.load(D + f"widx-{fam}.npz")
        m = W["minpos"] <= R
        p = SH + f"R{R}/{fam}-wm.npz"
        np.savez_compressed(p, M=W["M"][m], V=W["V"][m], pkg=W["pkg"][m].astype(np.uint16), normLen=W["normLen"][m].astype(np.uint32))
        row[f"{fam}.wm.rows"], row[f"{fam}.wm.MB"] = int(m.sum()), round(fsize(p) / 1e6, 3)
        T = np.load(D + f"uidx-{fam}.npz")
        for nm, mm in (("units", (T["nodes"] >= 30) & (T["content"] >= 3)), ("units20", T["nodes"] >= 20)):
            m = (T["minpos"] <= R) & mm
            p = SH + f"R{R}/{fam}-{nm}.npz"
            np.savez_compressed(p, h=T["h"][m], pkg=T["pkg"][m].astype(np.uint16))
            row[f"{fam}.{nm}.rows"], row[f"{fam}.{nm}.MB"] = int(m.sum()), round(fsize(p) / 1e6, 3)
            row[f"{fam}.{nm}.perKB"] = round(int(m.sum()) / max(code_kb[(fam, R)], 1), 3)
        J = np.load(D + f"jidx-{fam}.npz")
        em = J["minpos"] <= R
        erow = np.repeat(np.arange(len(em)), np.diff(J["ptr"]))
        sel = em[erow]
        ed, er = J["data"][sel], erow[sel]
        key = np.empty(len(ed), dtype=[("s", "u8"), ("p", "i4")]); key["s"], key["p"] = ed, J["pkg"][er]
        key = np.unique(key)
        voc, df = np.unique(key["s"], return_counts=True)
        kv = voc[df <= 2]
        k = np.searchsorted(kv, ed); k[k >= len(kv)] = 0
        keep = kv[k] == ed
        p = SH + f"R{R}/{fam}-b5.npz"
        cnt = np.bincount(er[keep], minlength=len(em))[em]
        np.savez_compressed(p, data=ed[keep], ptr=np.r_[0, np.cumsum(cnt)], pkg=J["pkg"][em].astype(np.uint16))
        row[f"{fam}.b5.entries"], row[f"{fam}.b5.shingles"], row[f"{fam}.b5.MB"] = int(em.sum()), int(keep.sum()), round(fsize(p) / 1e6, 3)
        row[f"{fam}.wm.perKB"] = round(row[f"{fam}.wm.rows"] / max(code_kb[(fam, R)], 1), 3)
    res.append(row)
    print(json.dumps(row), flush=True)

# literal index (bundler-independent): same filters as lt.py (held out, sponge regex, stop-lists)
SP = re.compile(r"(^|/)[^/]*(bundled|bundle\.|\.min\.|\.global\.|^cdn\.|/cdn\.|\.umd\.|browser)[^/]*$")
pos = json.load(open(D + "srcpos.json"))
content = {}
for l in open(D + "srclits-content.jsonl", encoding="utf8", errors="surrogatepass"):
    try:
        r = json.loads(l)
    except Exception:
        continue
    content[r["csha"]] = r["lits"]
files = {}
for l in open(D + "srcfiles.jsonl"):
    r = json.loads(l)
    if r["pkg"] in HELD or SP.search(r["file"]) or not content.get(r["csha"]):
        continue
    k = (r["pkg"], r["csha"])
    files[k] = min(files.get(k, 10**9), pos.get(r["pkg"] + "@" + r["ver"], 10**9))
for row in res:
    R = row["R"]
    ks = [k for k, v in files.items() if v <= R]
    vocab = {}
    ptr, data, pk = [0], [], []
    pid = {}
    for (p, c) in ks:
        ids = sorted({vocab.setdefault(w, len(vocab)) for w in content[c]})
        data.extend(ids); ptr.append(len(data)); pk.append(pid.setdefault(p, len(pid)))
    p = SH + f"R{R}/lits.npz"
    buf = io.BytesIO()
    np.savez_compressed(p, data=np.array(data, np.uint32), ptr=np.array(ptr, np.uint32), pkg=np.array(pk, np.uint16))
    vp = SH + f"R{R}/lits-vocab.txt.gz"
    with gzip.open(vp, "wt", encoding="utf8", errors="surrogatepass") as f:
        for w in vocab:
            f.write(w.replace("\n", "\\n") + "\n")
    row["lits.files"], row["lits.literals"], row["lits.pairs"] = len(ks), len(vocab), len(data)
    row["lits.MB"] = round((fsize(p) + fsize(vp)) / 1e6, 3)
    row["totalMB.bun"] = round(row["bun.wm.MB"] + row["bun.units.MB"] + row["bun.b5.MB"] + row["lits.MB"], 2)
    row["totalMB.both"] = round(row["totalMB.bun"] + row["esbuild.wm.MB"] + row["esbuild.units.MB"] + row["esbuild.b5.MB"], 2)
    print(R, "lits", row["lits.files"], row["lits.MB"], "total", row["totalMB.both"], flush=True)
json.dump(res, open(S + "results/index-size.json", "w"), indent=1)
