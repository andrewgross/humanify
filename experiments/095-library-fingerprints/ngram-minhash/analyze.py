"""Turn results/sim_*.npz into precision / recall / threshold tables.

  .venv/bin/python analyze.py [tag ...]      (default: every results/sim_*.npz)  -> results/analysis.json + stdout

Definitions (per query span, against the index of one bundler config):
  accepted at t      top score >= t (score = J, Jaccard; or CQ, share of the query found in the entry)
  pkg-correct        the top entry is from the query's package (or holds byte-identical source)
  file-correct       the top entry is the query's file (any version) or identical source
  exact              the top entry has the query's exact source (version-correct, or an unchanged copy)
  open-world FP      the query's best score against OTHER packages (different source) >= t: what would
                     be accepted if the query's own package were not in the index
  app FP             a Claude Code app negative (group or function) or lab app span scoring >= t
  CC positives       real Claude Code vendor modules; correct = top entry has their package (and for
                     highlight.js the grammar of the same name). Grammars absent from the lab index are
                     a second open-world negative set.
Only CROSS-CONFIG queries are scored for recall (the index config's own spans would match themselves).
"""
import glob
import json
import os
import re
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402
import srcsets  # noqa: E402

LAB = "/work/exp095-library-fingerprints/lab/"
SIZE_BUCKETS = [(0, 20), (20, 50), (50, 100), (100, 300), (300, 1000), (1000, 10 ** 9)]
TS = [0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]
_spans = None
_hljs = {}


def spans():
    global _spans
    if _spans is None:
        _spans = fp.load_spans()
    return _spans


def hljs_names(keys, cfg):
    """index key -> declared grammar name (name:"X") for highlight.js language files."""
    if cfg in _hljs:
        return _hljs[cfg]
    out = {}
    want = {k for k in keys if k.startswith("highlight.js\t") and "/languages/" in k}
    for r in spans():
        if r["set"] == "lab" and r["cfg"] == cfg and fp.file_key(r) in want and fp.file_key(r) not in out:
            # every declared name in the file (a grammar can embed sub-grammars with their own names)
            m = re.findall(r'\bname[:=]"([^"]+)"', open(r["path"], encoding="utf-8").read())
            if m:
                out[fp.file_key(r)] = set(m)
    _hljs[cfg] = out
    return out


def analyze(tag):
    R = dict(np.load(f"{fp.HERE}/results/sim_{tag}.npz"))
    idx = json.load(open(f"{fp.HERE}/results/index_{tag}.json"))
    keys = idx["keys"]
    cfg_index = tag.split("_", 1)[1]
    S = spans()
    src = fp.srcsha()
    ek = [k.split("\t") for k in keys]
    esrc = [src.get(k) for k in keys]
    st = np.array([r["set"] for r in S])
    cfg = np.array([r["cfg"] for r in S])
    mode = np.array([str(r["mode"]) for r in S])
    kind = np.array([str(r["kind"]) for r in S])
    ntok = np.array([r["ntok"] for r in S])
    qsrc = [src.get(fp.file_key(r)) if r["set"] == "lab" else None for r in S]
    out = {"tag": tag, "entries": len(keys), "vocab": idx.get("vocab"), "metrics": {}}
    names = hljs_names(keys, cfg_index)
    for M in ("J", "CQ"):
        topi = R[f"{M}_topi"]
        top = R[f"{M}_top"]
        pkg_ok = np.array([bool(S[i]["pkg"] == ek[t][0] or (qsrc[i] and qsrc[i] == esrc[t])) for i, t in enumerate(topi)])
        file_ok = np.array([bool((S[i]["pkg"] == ek[t][0] and S[i]["file"] == ek[t][2]) or (qsrc[i] and qsrc[i] == esrc[t]))
                            for i, t in enumerate(topi)])
        exact = np.array([bool(qsrc[i]) and qsrc[i] == esrc[t] for i, t in enumerate(topi)])
        in_index = (R["J_same"] >= 0) | (R["J_samefile"] >= 0)
        other = R[f"{M}_other"]
        oi = R[f"{M}_otheri"]
        # an open-world hit is SPURIOUS unless the two source files genuinely share code
        spurious = np.zeros(len(S), bool)
        for i in np.nonzero((other >= min(TS)) & (st == "lab"))[0]:
            spurious[i] = not srcsets.genuine(fp.file_key(S[i]), keys[oi[i]])
        res = {}
        lab = (st == "lab") & (cfg != cfg_index)
        groups = {"all-cross": lab, "full-modules": lab & (mode != "some"), "tree-shaken(some)": lab & (mode == "some")}
        for c in ("bun-1.3.14", "bun-1.4.3", "esbuild-0.28.2", "rollup-4.64.5", "bun-1.4.0"):
            if c != cfg_index:
                groups["cfg:" + c] = lab & (cfg == c) & (mode != "some")
        for k in ("cjs", "lazy-esm", "eager-esm"):
            groups["kind:" + k] = lab & (kind == k) & (mode != "some")
        for lo, hi in SIZE_BUCKETS:
            groups[f"size:{lo}-{hi}"] = lab & (ntok >= lo) & (ntok < hi) & (mode != "some")
        cc_pos = st == "cc-pos"
        pos_ok = np.zeros(len(S), bool)
        pos_in = np.zeros(len(S), bool)
        for i in np.nonzero(cc_pos)[0]:
            r = S[i]
            t = topi[i]
            if r["pkg"] == "highlight.js" and not str(r["file"]).startswith("vendor/") and r["file"] != "package.json":
                pos_in[i] = any(r["file"] in v for v in names.values())
                pos_ok[i] = ek[t][0] == "highlight.js" and r["file"] in names.get(keys[t], ())
            else:
                pos_in[i] = any(e[0] == r["pkg"] for e in ek)
                pos_ok[i] = ek[t][0] == r["pkg"]
        negs = {"cc-neg": st == "cc-neg", "cc-negfn": st == "cc-negfn", "lab-app": st == "lab-app",
                "cc-pos-not-in-index": cc_pos & ~pos_in}
        for t in TS:
            row = {}
            for g, m in groups.items():
                acc = m & (top >= t)
                n_in = (m & in_index).sum()
                row[g] = {
                    "n": int(m.sum()),
                    "accepted": round(float(acc.sum() / max(m.sum(), 1)), 4),
                    "prec_pkg": round(float(pkg_ok[acc].mean()), 4) if acc.any() else None,
                    "prec_file": round(float(file_ok[acc].mean()), 4) if acc.any() else None,
                    "recall_file": round(float((acc & file_ok & in_index).sum() / max(n_in, 1)), 4),
                    "recall_exact": round(float((acc & exact).sum() / max((m & (R["J_same"] >= 0)).sum(), 1)), 4),
                    "openworld_fp": round(float((m & (other >= t)).sum() / max(m.sum(), 1)), 5),
                    "openworld_fp_spurious": round(float((m & (other >= t) & spurious).sum() / max(m.sum(), 1)), 5),
                }
            row["negatives"] = {g: [int((m & (top >= t)).sum()), int(m.sum())] for g, m in negs.items()}
            pin = cc_pos & pos_in
            row["cc-pos-in-index"] = {"n": int(pin.sum()), "accepted_correct": int((pin & (top >= t) & pos_ok).sum()),
                                      "accepted_wrong": int((pin & (top >= t) & ~pos_ok).sum())}
            res[str(t)] = row
        # same-module similarity percentiles by config (full modules)
        sim = {}
        for g, m in groups.items():
            v = R[f"{M}_same"][m & (R["J_same"] >= 0)]
            o = other[m]
            if len(v):
                sim[g] = {"same_p5_25_50": [round(float(x), 3) for x in np.percentile(v, [5, 25, 50])],
                          "other_p50_99_999": [round(float(x), 3) for x in np.percentile(o, [50, 99, 99.9])]}
        res["similarity"] = sim
        for g, m in negs.items():
            if m.any():
                res["similarity"]["neg:" + g] = {"top_p50_99_max": [round(float(x), 3) for x in np.percentile(top[m], [50, 99, 100])]}
        if f"{M}_nother50" in R:
            # expected false hits per query per 1,000 unrelated index entries (open world, by size)
            fh = {}
            n_other = len(keys)
            for lo, hi in SIZE_BUCKETS:
                m = groups[f"size:{lo}-{hi}"]
                fh[f"{lo}-{hi}"] = {str(t): round(float(R[f"{M}_nother{int(t * 100)}"][m].mean() / n_other * 1000), 5) for t in TS}
            res["false_hits_per_1000_entries"] = fh
        out["metrics"][M] = res
    return out


def main():
    tags = sys.argv[1:] or sorted(os.path.basename(p)[4:-4] for p in glob.glob(f"{fp.HERE}/results/sim_*.npz"))
    allres = {}
    path = f"{fp.HERE}/results/analysis.json"
    if os.path.exists(path):
        allres = json.load(open(path))
    for tag in tags:
        allres[tag] = analyze(tag)
        a = allres[tag]["metrics"]["J"]
        line = []
        for t in ("0.5", "0.7"):
            r = a[t]
            line.append(f"J>={t}: rec(full) {r['full-modules']['recall_file']:.3f} prec {r['full-modules']['prec_pkg']} "
                        f"ow {r['full-modules']['openworld_fp']:.4f} esb {r.get('cfg:esbuild-0.28.2', r.get('cfg:bun-1.4.0'))['recall_file']:.3f} "
                        f"rollup {r['cfg:rollup-4.64.5']['recall_file']:.3f} neg {sum(v[0] for k, v in r['negatives'].items() if k != 'cc-pos-not-in-index')} "
                        f"ccpos {r['cc-pos-in-index']['accepted_correct']}/{r['cc-pos-in-index']['n']} w{r['cc-pos-in-index']['accepted_wrong']}")
        print(tag, "|", " | ".join(line), flush=True)
    json.dump(allres, open(path, "w"), indent=1)


if __name__ == "__main__":
    main()
