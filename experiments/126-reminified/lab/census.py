"""exp122 step 1e: the class-d census — every class-d library byte of the dev label gets ONE cause, as points of
the set's library bytes. Reads the private per-unit files of plan.py, diff.py and analyze.py; writes aggregates
only (experiments/122-hash-differs/census.json) and prints the table.

Causes, first that applies:
  no-build        no recipe/4 build of the package@version in the slice is on disk (a measurement gap)
  norm:<X>        a candidate normalisation alone lets the right package's rows cover the unit (exact: the lab
                  re-fingerprints both sides), in the order K T C P M D A L I O
  norm:combined   only the combination of all of them does
  not-emitted     no counterpart of comparable code in the index's own build of that package@version (best
                  token-diff ratio < 0.5, or no unit of that package@version at all): the bundle reached code the
                  index's entry did not (another entry file / export condition / CJS-vs-ESM file / tree-shaking)
  near:<feature>  a counterpart with ratio >= 0.8 that no normalisation equalises, by the first diff feature:
                  lazy-init (declaration vs assignment), interop (member access on an import binding, `(0, f)`),
                  inlined (a binding vs an inlined literal), other
  partial         a counterpart with ratio 0.5-0.8 (the same code, substantially differently emitted)
"""
import collections
import gzip
import json
import os
import re

D = "/work/exp126-data/lab"
HERE = os.path.dirname(os.path.abspath(__file__))
SINGLE = list("KTCPMDALIO")
COMBINED = "KTCPMDALIO"
LIB = {"wrangler": 730596, "remin-esbuild": 7216578, "remin-terser": 7369545}


def recovered(v):
    p = f"{D}/recovered-{v}.jsonl.gz"
    return {tuple(x) for x in map(json.loads, gzip.open(p, "rt"))} if os.path.exists(p) else set()


def feature(ops):
    text = " ".join(f"{a} -> {b}" for _, a, b in ops)
    if re.search(r"VariableDeclarator.*AssignmentExpression|AssignmentExpression.*VariableDeclarator|"
                 r"VariableDeclaration -> ExpressionStatement|ExpressionStatement -> VariableDeclaration", text):
        return "lazy-init"
    if "MemberExpression{computed:false;object:" in text or "SequenceExpression{expressions:[N=0" in text:
        return "interop"
    if re.search(r"\$\d* -> [NS]=|[NS]=[^ ]* -> \$", text):
        return "inlined"
    return "other"


def main():
    rec = {v: recovered(v) for v in SINGLE + [COMBINED]}
    tot = collections.defaultdict(collections.Counter)
    for line in gzip.open(f"{D}/diffs.jsonl.gz", "rt"):
        r = json.loads(line)
        k = (r["entry"], r["s"], r["e"])
        st = r["set"]
        if r["status"] == "noBuild":
            c = "no-build"
        else:
            c = next((f"norm:{v}" for v in SINGLE if k in rec[v]), None)
            if c is None and k in rec[COMBINED]:
                c = "norm:combined"
            if c is None:
                ratio = r.get("ratio", 0)
                if r["status"] in ("noQuery", "noCounterpart") or ratio < 0.5:
                    c = "not-emitted"
                elif ratio >= 0.8:
                    c = "near:" + feature(r["ops"])
                else:
                    c = "partial"
        tot[st][c] += r["b"]
        tot[st]["d"] += r["b"]
    out = {s: {k: round(100 * v / LIB[s], 3) for k, v in sorted(c.items())} for s, c in sorted(tot.items())}
    json.dump({"unit": "points of the set's library bytes (scorer 9, exp119-k2all-st labels)", "sets": out},
              open(os.path.join(HERE, "..", "census.json"), "w"), indent=1, sort_keys=True)
    keys = sorted({k for c in out.values() for k in c})
    print("set".ljust(13) + "".join(k[:13].rjust(14) for k in keys))
    for s, c in out.items():
        print(s.ljust(13) + "".join(f"{c.get(k, 0):14.2f}" for k in keys))


if __name__ == "__main__":
    main()
