"""exp095 "misses": sub-classify class-d diffs (diffd.py output) by the shape of the canonical-form difference.

usage: python3 -I dsub.py <set> [--show <subclass> [n]]   -> totals by subclass; with --show, examples to read

Subclasses (first that applies; derived from reading the diffs by hand, README §2):
  unit-model     the query unit is a whole CommonJS module (minified esbuild `__commonJS({"path"(e,t){…}})` the unpack
                 did not recognise, so it stayed one statement; the index splits module bodies into statements)
  row-missing    the index's OWN build holds the identical unit (same hash) but the index has no row for it
  no-build       no kept build of that package version to diff against
  no-counterpart nothing of that file in the index's build near the unit's position
  decl-vs-assign same code emitted as a declaration on one side and an assignment on the other (`var x=e` vs `x=e`,
                 `var a,b; a=…,b=…` in a lazy ESM init vs one `var` in a CommonJS factory or an eager module)
  lazy-vs-eager  one side wraps the code in a lazy-init function (`__esm` / `__commonJS` body) the other emits eagerly
  minifier-syntax `const`↔`let`, declarations joined/split (`let a;let b` vs `let a,b`), a constant inlined as a number
  interop        `x.default` / namespace member wrapping differs (ESM-CJS interop of an import)
  string-escape  only a string's raw spelling differs (`\\uXXXX` escapes vs UTF-8: Bun's ASCII output)
  numbers        only numeric literals differ (inlined or folded constants, build-machine values)
  catch-param    an unused `catch(e)` binding dropped on one side (optional catch binding)
  export-table   Bun's generated `__export(ns, {name: () => local, …})` registry: generated code the map credits
                 to whichever file it likes (truth-side noise as much as a miss)
  string-constant a string constant of a different length (build paths, `import.meta.url` shims, defines)
  low-similarity the closest index unit shares < 50% of tokens: a different slice of the code (tree-shaken,
                 inlined, moved), not a rewrite of the same statement
  other          anything else (read by hand)
"""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mmlib import MM  # noqa: E402

DECL = {"VariableDeclaration", "VariableDeclarator", "ExpressionStatement"}


def only(ops, pred):
    return ops and all(pred(o) for o in ops)


def sub(r):
    if r["verdict"] == "no-build":
        return "no-build"
    if r["verdict"] in ("no-counterpart", "no-query-canon"):
        return "no-counterpart"
    if r.get("cjsWhole"):
        return "unit-model"
    if r.get("same"):
        return "row-missing"
    ops = r["ops"]
    if r.get("exportTable"):
        return "export-table"
    if r["qType"] != r["iType"] and {r["qType"], r["iType"]} <= DECL and r["ratio"] >= 0.25:
        return "decl-vs-assign"
    if r["ratio"] < 0.5:
        return "low-similarity"
    if any(o[0] == "insert" and (o[2].startswith(": CallExpression { arguments : [ FunctionExpression")
                                 or o[2].startswith("FunctionExpression { async : false ; body : BlockStatement { body : "
                                                    "[ ExpressionStatement { expression : CallExpression { arguments : [ ] ;"))
           for o in ops[:2]):
        return "lazy-vs-eager"
    kinds = {'"const"', '"let"', '"var"'}

    def minify_op(o):
        return ((o[1] in kinds and o[2] in kinds) or "} , VariableDeclaration { declarations : [" in o[1] + o[2]
                or (o[1].startswith("$") and o[2].startswith("N=")) or (o[1].startswith("N=") and o[2].startswith("$")))
    if any(minify_op(o) for o in ops):
        return "minifier-syntax"
    if any(t in o[1] or t in o[2] for o in ops for t in ("I=default", "I=require", "I=__esModule")) or only(
            ops, lambda o: o[0] == "insert" and ("MemberExpression { computed : false ; object :" in o[2]
                                                 or o[2].startswith("; optional : false ; property : I="))):
        return "interop"
    if only(ops, lambda o: o[0] == "replace" and o[1].startswith("S=__STR_") and o[2].startswith("S=__STR_")):
        return "string-constant"
    if only(ops, lambda o: o[0] == "replace" and o[1].startswith("Q=") and o[2].startswith("Q=")):
        return "string-escape"
    if only(ops, lambda o: o[0] == "replace" and o[1].startswith("N=") and o[2].startswith("N=")):
        return "numbers"
    if any("param : $" in o[1] or "param : $" in o[2] for o in ops) and r["nops"] <= 40:
        return "catch-param"
    return "other"


def main():
    which = sys.argv[1]
    rs = [json.loads(l) for l in open(f"{MM}/diffd/{which}.jsonl")]
    c, n = collections.Counter(), collections.Counter()
    for r in rs:
        r["sub"] = sub(r)
        c[r["sub"]] += r["b"]
        n[r["sub"]] += 1
    tot = sum(c.values())
    print(which, f"class-d bytes {tot:,}")
    for k, v in c.most_common():
        print(f"  {k:15s} {v:10,} {v / tot:6.1%}  units {n[k]}")
    json.dump({"bytes": dict(c), "units": dict(n)}, open(f"{MM}/diffd/{which}.sub.json", "w"))
    if "--show" in sys.argv:
        i = sys.argv.index("--show")
        want = sys.argv[i + 1]
        k = int(sys.argv[i + 2]) if len(sys.argv) > i + 2 else 8
        for r in sorted((r for r in rs if r["sub"] == want), key=lambda r: -r["b"])[:k]:
            print("##", r["b"], r.get("app"), r["pkg"], r["v"], r.get("file"), r.get("qKind"), r.get("iKind"),
                  r.get("qType"), r.get("iType"), r.get("qNodes"), r.get("iNodes"), r.get("ratio"), r.get("nops"))
            for o in r.get("ops", [])[:5]:
                print("    ", o[0], "| Q:", o[1][:200], "| I:", o[2][:200])


if __name__ == "__main__":
    main()
