"""Two-family agreement: structural AND literal (or one + a declared reader), vs each family alone.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/combine.py [R list]  -> results/combine.json (+ printed table)
Components (thresholds as re-derived at scale; see README):
  WM  whole-module exact: normLen >= 400, one package/family holds the hash, normV agrees, string-bearing, stop-lists
  JC  B5 Jaccard: filter d=2, J >= 0.7, margin 0.1, >= 20 informative runs
  UV  unit votes: >= 30 nodes, >= 3 content tokens, family-merged owners, stop-lists + specificity, k >= 2
  S   structural consensus: the claims of WM / JC / UV that exist must agree (family); name = WM > JC > UV
  L   literal containment from npm source: specific files only, stop-lists, T 20 bits, cover 0.9, margin 10, > 5 literals
  Rd  declared reader (banner / inlined package.json / scoped error prefix)
Loose components (to test whether agreement buys looser thresholds): UV k>=1, JC J>=0.5, L cover >= 0.7.
"""
import json, pickle, sys, collections
from lib import *
from evalq import Q, metrics, summary, classify_set
from claims import jc_claims, lt_claims, uv_load, uv_claims

STOP = "helpers+runtime+stainless+shim+spec"
RD = pickle.load(open(D + "readers.pkl", "rb"))
WMC = pickle.load(open(D + "wm-claims.pkl", "rb"))


def components(fam, R):
    c = {}
    c["WM"] = WMC[(fam, R, 400, 1, "all")]
    c["JC"] = jc_claims(fam, R, 2, 0.7, 0.1, 20)
    c["JCl"] = jc_claims(fam, R, 2, 0.5, 0.1, 20)
    v = uv_load(fam, R, 30, 3, True, STOP)
    c["UV"] = uv_claims(v, 2)
    c["UVl"] = uv_claims(v, 1)
    c["L"] = lt_claims(R, 20, 0.9, 10, 6, specific=True)
    c["Ll"] = lt_claims(R, 20, 0.7, 10, 6, specific=True)
    c["Rd"] = {s: p for s, (p, how) in RD.items()}
    return c


def consensus(*claims):
    xs = [x for x in claims if x]
    if not xs:
        return None
    if all(same(x, xs[0]) for x in xs[1:]):
        return xs[0]
    return None


def agree(a, b, prefer=None):
    if a and b and same(a, b):
        # name: the more specific (a real package name beats a family name)
        for x in (prefer, a, b):
            if x and not x.startswith(("fam:", "scope:", "base:")):
                return x
        return a
    return None


def variants(c, s):
    wm, jc, uv, L, rd = c["WM"].get(s), c["JC"].get(s), c["UV"].get(s), c["L"].get(s), c["Rd"].get(s)
    S = consensus(wm, jc, uv)
    Sl = consensus(wm, c["JCl"].get(s), c["UVl"].get(s))
    Ll = c["Ll"].get(s)
    out = dict(WM=wm, JC=jc, UV=uv, S=S, L=L, Rd=rd)
    out["S|L"] = consensus(S, L)
    out["S&L"] = agree(S, L, L)
    out["S&Rd"] = agree(S, rd, rd)
    out["L&Rd"] = agree(L, rd, rd)
    out["design"] = out["S&L"] or out["S&Rd"] or out["L&Rd"]
    out["Sl&Ll"] = agree(Sl, Ll, Ll)
    out["design-loose"] = out["Sl&Ll"] or agree(Sl, rd, rd) or agree(Ll, rd, rd)
    out["WM|S&L"] = consensus(wm, out["design"]) if (wm or out["design"]) else None  # exact module hash alone OR agreement
    return out


if __name__ == "__main__":
    Rs = [int(x) for x in sys.argv[1].split(",")] if len(sys.argv) > 1 else SCALES
    res = []
    for R in Rs:
        for fam in ("bun", "esbuild"):
            c = components(fam, R)
            V = {}
            for q in Q():
                if q["fam"] == fam and q["sha"] not in V:
                    V[q["sha"]] = variants(c, q["sha"])
            pickle.dump(V, open(D + f"combine-{fam}-R{R}.pkl", "wb"))
            for name in ("WM", "JC", "UV", "S", "L", "Rd", "S|L", "S&L", "S&Rd", "L&Rd", "design", "Sl&Ll", "design-loose", "WM|S&L"):
                m, ex = metrics(lambda q: V.get(q["sha"], {}).get(name), R, famq=(fam,))
                r = dict(variant=name, fam=fam, R=R, **summary(m), foreignNonFamily=m["Fn"] - m["Fcopy"])
                r["examples"] = ex["F"][:100] + [("APP",) + x for x in ex["A"][:30]] + [("PWRONG",) + x for x in ex["Pwrong"][:40]]
                res.append(r)
                print(f'{fam} R{R} {name:14s} | foreign named {r["foreignNamed"]} ({r["foreignRate"]}, non-family {r["foreignNonFamily"]}) | recall {r["recall"]} strict {r["recallStrict"]} exactVer {r["recallExactVer"]} otherVer {r["recallOtherVer"]} wrong {r["wrongPkg"]} | app {r["appGroupsNamed"]} {r["appFnsNamed"]}', flush=True)
    json.dump(res, open(S + f"results/combine{'-' + sys.argv[1].replace(',', '_') if len(sys.argv) > 1 else ''}.json", "w"), indent=0)
