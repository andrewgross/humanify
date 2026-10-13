"""exp095 "misses": class d (version indexed, hash differs) — find the SAME source code in the index's own build of
that package version and diff the two canonical forms the unit hash is taken of.

usage: <venv python> -I diffd.py heldout|wrangler [max package-versions]   -> $MM/diffd/<set>.jsonl
For every class-d unit (structural rule rows of decompose.py): the index build of <pkg>@<version> (recipe/4 builds kept
by the indexv2 lane's cost index, else the selection lane's) is dumped index-side with canonical forms; each index
unit is attributed through the build's map (package, file, original line range); the index unit holding the query
unit's original start position in the same file is its counterpart. Both canonical forms are tokenised and diffed.
"""
import bisect
import hashlib
import collections
import difflib
import json
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mmlib import MM  # noqa: E402
from truth import package_root, segments  # noqa: E402

DUMP = f"{MM}/target/release/exp095-unit-dump"
WORKS = ["/work/exp095-index-v2-data/selABC/work", "/work/exp095-library-fingerprints/selection/work", f"{MM}/work"]
QUERY = {"heldout": None, "wrangler": "/work/exp095-index-v2-data/eval/wrangler/cli.min.js",
         "cc": "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js"}


def build_dirs(pkg, ver, slice_):
    for w in WORKS:
        d = f"{w}/{pkg.replace('/', '__')}@{ver}/build/{slice_}"
        if os.path.isdir(d):
            return [f"{d}/{e}" for e in ("req", "dyn") if os.path.exists(f"{d}/{e}/out.js")]
    return []


def index_units(bdir, slice_):
    """Index-side units of one build with canon, attributed: [(unit, pkg, file, (lo, hi) original positions)]."""
    out_js = f"{bdir}/out.js"
    cache = f"{MM}/diffd/cache/{hashlib.md5(bdir.encode()).hexdigest()}.jsonl"
    os.makedirs(os.path.dirname(cache), exist_ok=True)
    if not os.path.exists(cache):
        subprocess.run([DUMP, out_js, cache, "--side", "index", "--bundler", slice_, "--canon", "all"], check=True)
    units = [json.loads(l) for l in open(cache)]
    text = open(out_js, encoding="utf8").read()
    smap = json.load(open(out_js + ".map"))
    segs = segments(text, smap)
    offs = [s[0] for s in segs]
    res = []
    for u in units:
        if "cw" not in u:
            continue
        k = max(bisect.bisect_right(offs, u["s"]) - 1, 0)
        by = collections.Counter()
        pos = collections.defaultdict(list)
        while k < len(segs) and segs[k][0] < u["e"]:
            a, b = max(segs[k][0], u["s"]), min(segs[k + 1][0] if k + 1 < len(segs) else u["e"], u["e"])
            if b > a:
                by[segs[k][1]] += b - a
                pos[segs[k][1]].append(tuple(segs[k][2:]))
            k += 1
        if not by:
            continue
        src = by.most_common(1)[0][0]
        pr = package_root(smap["sources"][src])
        if not pr:
            continue
        res.append((u, pr[0], pr[2], (min(pos[src]), max(pos[src]))))
    return res


TOK = re.compile(r"[A-Za-z]+(?:=[^;,{}\[\]]*)?|\$\d+|[{}\[\];,:]|\"[^\"]*\"|[^\s]")


def toks(c):
    return TOK.findall(c)


def diff(a, b, limit=6):
    """Compact token diff of query canon `a` vs index canon `b`: [(op, query tokens, index tokens)]."""
    ta, tb = toks(a), toks(b)
    sm = difflib.SequenceMatcher(None, ta, tb, autojunk=False)
    ops = [(op, " ".join(ta[i1:i2])[:300], " ".join(tb[j1:j2])[:300]) for op, i1, i2, j1, j2 in sm.get_opcodes()
           if op != "equal"]
    return round(sm.ratio(), 4), len(ops), ops[:limit]


CJS_WHOLE = re.compile(r'^(?:VariableDeclarator\{id:\$\d+;init:|.{0,200}?init:)CallExpression\{arguments:\[ObjectExpression'
                       r'\{properties:\[Property\{computed:false;key:S=__STR_\d+__;kind:"init";method:true')


EXPORT_TABLE = re.compile(r'^ExpressionStatement\{expression:CallExpression\{arguments:\[\$\d+,ObjectExpression\{properties:'
                          r'\[Property\{computed:false;key:[^;]+;kind:"init";method:false;value:ArrowFunctionExpression\{async:false;'
                          r'body:\$\d+;')


def best_counterpart(qc, cands):
    """The candidate whose canonical form is closest to the query's (token similarity), smallest first on ties."""
    qt = toks(qc)
    best, key = None, None
    for x in sorted(cands, key=lambda x: x[1]["e"] - x[1]["s"])[:40]:
        it = toks(x[1]["cw"])
        sm = difflib.SequenceMatcher(None, qt, it, autojunk=False)
        k = sm.quick_ratio() if max(len(qt), len(it)) > 20000 else sm.ratio()
        if key is None or k > key:
            best, key = x, k
    return best[0], best[1]


def top_type(c):
    return c.split("{", 1)[0]


def main():
    which = sys.argv[1]
    limit = int(sys.argv[2]) if len(sys.argv) > 2 else 10 ** 9
    slice_ = "esbuild" if which == "wrangler" else "bun"
    rows = [json.loads(l) for l in open(f"{MM}/decomp/{which}.rows.jsonl")]
    d = [r for r in rows if r["rule"] == "struct" and r["cls"] in ("d-file", "d-variant", "d-absent", "d-nofile")]
    by_pv = collections.defaultdict(list)
    for r in d:
        by_pv[(r["pkg"], r["v"])].append(r)
    order = sorted(by_pv, key=lambda k: -sum(r["b"] for r in by_pv[k]))[:limit]
    # query canon, per bundle
    by_bundle = collections.defaultdict(list)
    for k in order:
        for r in by_pv[k]:
            by_bundle[r.get("app", which)].append(r)
    qcanon = {}
    apps = {s: js for s, js, _ in json.load(open("/work/exp095-index-v2-data/eval/heldout-apps.json"))}
    os.makedirs(f"{MM}/diffd", exist_ok=True)
    for b, rs in by_bundle.items():
        js = QUERY[which] or apps[b]
        st = f"{MM}/diffd/q-{b}.starts"
        open(st, "w").write("\n".join(str(r["s"]) for r in rs) + "\n")
        out = f"{MM}/diffd/q-{b}.jsonl"
        subprocess.run([DUMP, js, out, "--canon", st], check=True)
        for l in open(out):
            u = json.loads(l)
            if "cw" in u:
                qcanon[(b, u["s"])] = u
    with open(f"{MM}/diffd/{which}.jsonl", "w") as fh:
        for k in order:
            dirs = build_dirs(k[0], k[1], slice_)
            ius = []
            for bd in dirs:
                ius += [(bd.rsplit("/", 1)[1],) + x for x in index_units(bd, slice_)]
            for r in by_pv[k]:
                q = qcanon.get((r.get("app", which), r["s"]))
                rec = {k2: r.get(k2) for k2 in ("app", "s", "b", "pkg", "v", "file", "pos", "cls")}
                if not dirs:
                    rec["verdict"] = "no-build"
                elif q is None:
                    rec["verdict"] = "no-query-canon"
                else:
                    if r.get("file") is None:  # Claude Code: no map — every unit of the package is a candidate
                        same = cands = [x for x in ius if x[2] == r["pkg"]]
                        cands = sorted(cands, key=lambda x: -difflib.SequenceMatcher(
                            None, q["cw"][:4000], x[1]["cw"][:4000], autojunk=False).quick_ratio())[:40]
                        pos = None
                    else:
                        pos = tuple(r["pos"])
                        cands = [x for x in ius if x[2] == r["pkg"] and x[3] == r["file"] and x[4][0] <= pos <= x[4][1]]
                        same = [x for x in ius if x[2] == r["pkg"] and x[3] == r["file"]]
                    if not cands and pos is not None:  # nearest start at or before the position, else the first after it (same file)
                        cands = sorted([x for x in same if x[4][0] <= pos], key=lambda x: x[4][0])[-1:]
                    if not cands and pos is not None:
                        cands = sorted([x for x in same if x[4][0] > pos], key=lambda x: x[4][0])[:1]
                    if not cands:
                        rec["verdict"] = "no-counterpart"
                        rec["indexFiles"] = sorted({x[3] for x in ius if x[2] == r["pkg"]})[:8]
                    else:
                        near = [x for x in same if pos is not None and (x[4][0] <= pos <= x[4][1] or abs(x[4][0][0] - pos[0]) <= 2)]
                        build, iu = best_counterpart(q["cw"], near or cands)
                        ratio, nops, ops = diff(q["cw"], iu["cw"])
                        rec.update(cjsWhole=bool(CJS_WHOLE.match(q["cw"])), exportTable=bool(EXPORT_TABLE.match(q["cw"])), sameFileUnits=len(same),
                                   indexBodyNodes=sum(x[1]["w"][1] for x in same if x[0] == build))
                        rec.update(verdict="diffed", build=build, qKind=q["k"], iKind=iu["k"], qType=top_type(q["cw"]),
                                   iType=top_type(iu["cw"]), qNodes=q["w"][1], iNodes=iu["w"][1], same=q["w"][0] == iu["w"][0],
                                   ratio=ratio, nops=nops, ops=ops, qLen=len(q["cw"]), iLen=len(iu["cw"]))
                fh.write(json.dumps(rec) + "\n")
    print("done", len(order), "package versions")


if __name__ == "__main__":
    main()
