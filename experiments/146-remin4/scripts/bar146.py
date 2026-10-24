"""exp146: exp145's two-check terser dev bar and 36-app subsample bootstrap, applied to exp144's candidate with the
third terser dev corpus remin4 (144 apps) added. DEV labels only; judge sets are refused by exp145's loaders.

usage: python3 bar146.py <out.json>
Check 1: p05 of the 36-app precision delta (20,000 draws without replacement, seed 145) >= 0, read on remin4-terser
alone, on exp145's pooled remin + remin3 terser, and on all three pooled.
Check 2: T36 = (1 - P_base) x (new named bytes per 36 pooled terser apps) must exceed the candidate's largest
single-bundle wrong gain (wrong family + app claimed) on ANY dev set (exp145's dev label pairs + remin4's two sets).
Writes app-free, package-free numbers."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../145-terser-predictivity/scripts"))
from all145 import CANDS, WIDE, boot, rows, stats  # noqa: E402
from pairs145 import load  # noqa: E402

R146 = os.path.normpath(os.path.join(HERE, "../../096-library-matching-score/results"))
NAME = "exp144 veto_support=shown"
BASE4, CAND4 = "remin4-exp142-asof", "remin4-exp144-shown2"


def max_add(pairs):
    m, where = 0.0, None
    for results, base, cand in pairs:
        b, c = load(results, base), load(results, cand)
        for s in b:
            for a in b[s]:
                d = (c[s][a][1] - b[s][a][1]) / 1e3
                if d > m:
                    m, where = d, s
    return m, where


def adds(pairs):
    out = []
    for results, base, cand in pairs:
        b, c = load(results, base), load(results, cand)
        for s in b:
            out += [(c[s][a][1] - b[s][a][1]) / 1e3 for a in b[s]]
    return out


def tol36(st):
    return (1 - st["P_base"]) * st["dNamedKB"] / st["apps"] * 36


def main(out):
    corp = dict(CANDS[NAME][2])
    corp["remin4"] = (R146, BASE4, CAND4)
    res = {"candidate": NAME, "sets": {}, "pooled": {}}
    terser = {}
    for corpus, (results, base, cand) in corp.items():
        for kind in ("terser", "esbuild"):
            rs = rows(results, base, cand, f"{corpus}-{kind}")
            res["sets"][f"{corpus}-{kind}"] = {**stats(rs), "boot": boot(rs)}
            if kind == "terser":
                terser[corpus] = rs
    for tag, keys in (("remin+remin3 (exp145)", ("remin", "remin3")), ("remin4", ("remin4",)),
                      ("remin+remin3+remin4", ("remin", "remin3", "remin4"))):
        rs = [r for k in keys for r in terser[k]]
        st = {**stats(rs), "boot": boot(rs)}
        st["T36KB"] = tol36(st)
        res["pooled"][tag] = st
    pairs = [v for v in corp.values()] + WIDE[NAME]
    m, where = max_add(pairs)
    m4, where4 = max_add([corp["remin4"]])
    m_old, where_old = max_add([v for k, v in corp.items() if k != "remin4"] + WIDE[NAME])
    res["maxBundleAddKB"] = {"anyDev": m, "anyDevSet": where, "remin4": m4, "remin4Set": where4,
                             "exp145DevOnly": m_old, "exp145DevOnlySet": where_old}
    every = adds(pairs)
    res["devBundles"] = len(every)
    verdicts = {}
    for tag, st in res["pooled"].items():
        c1 = st["boot"]["p05"] >= 0
        c2 = st["T36KB"] > m
        hit = sum(1 for x in every if x >= st["T36KB"])
        q = hit / len(every)
        verdicts[tag] = {"check1_p05": st["boot"]["p05"], "check1": c1, "T36KB": st["T36KB"], "maxAddKB": m,
                         "check2": c2, "bar": "pass" if c1 and c2 else "reject", "bundlesAtOrAboveT36": hit,
                         "pFail36": 1 - (1 - q) ** 36}
    res["bar"] = verdicts
    json.dump(res, open(out, "w"), indent=1, sort_keys=True)
    print(f"{'set':<24}{'apps':>5}{'P_base':>9}{'dP pts':>9}{'dNamed KB':>11}{'dWrong KB':>10}{'added KB':>9}{'p05':>9}{'p50':>9}{'<0':>7}")
    for tag, s in list(res["sets"].items()) + list(res["pooled"].items()):
        b = s["boot"]
        print(f"{tag:<24}{s['apps']:>5}{100*s['P_base']:>9.3f}{100*s['dP']:>9.4f}{s['dNamedKB']:>11.0f}{s['dWrongKB']:>10.2f}"
              f"{s['addedWrongKB']:>9.2f}{100*b['p05']:>9.4f}{100*b['p50']:>9.4f}{b['below0']:>7.3f}")
    print("largest single-bundle wrong add:", res["maxBundleAddKB"])
    for tag, v in verdicts.items():
        print(f"bar on {tag:<22} check1 p05 {100*v['check1_p05']:+.4f} {'pass' if v['check1'] else 'fail'}; "
              f"check2 T36 {v['T36KB']:.2f} KB vs max {v['maxAddKB']:.2f} KB {'pass' if v['check2'] else 'fail'} -> {v['bar']}; "
              f"{v['bundlesAtOrAboveT36']}/{res['devBundles']} dev bundles >= T36, P(fail36) {v['pFail36']:.3f}")


if __name__ == "__main__":
    main(sys.argv[1])
