"""exp123 step 1c: for the not-emitted units whose truth FILE the package's own index build did emit (reach.py
`file-emitted`), why the index holds no matching row.

Per (slice, package, version, file):
  sponge:vendored-path   the builder's sponge_evidence drops every unit of the file: a vendor/third_party dir
  sponge:bundler-marker  ... a module-marker comment (`//#region node_modules/x/...`, esbuild `// node_modules/...`,
                         webpack `/***/ "..."`) names ANOTHER package (crates/humanify-index/src/build.rs,
                         `sponge_evidence`, emulated here line for line)
  rows:none              not a sponge, and the index holds no row of that file@version in the slice (attribution:
                         mixed / impure spans, a file credited to two modules)
  rows:some              rows of that file@version exist; this statement's print is not among them (tree-shaken
                         or emitted differently in the index's build)
For bundler-marker files, the unit's own position is placed against the file's foreign regions: whether the APP's
unit falls inside a region naming another package is read from the truth (`origin`) of exp122's capture.

usage: python3 -I why_dropped.py     (reads reach.jsonl.gz, ne-units.jsonl.gz; k2-all unit tables) -> aggregates
"""
import collections
import gzip
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from reach import find_root, own_build  # noqa: E402

D = "/work/exp123-data"
IX = "/work/exp113-data/ix/k2-all"
MAXB = 4_000_000
VENDORED = {"vendor", "vendored", "third_party", "third-party"}
EXTS = (".js", ".mjs", ".cjs", ".ts", ".mts", ".cts", ".jsx", ".tsx", ".json")
LIB = {"heldout": 30358072, "esbuild-app": 4916988, "wrangler": 730596, "apps-esbuild": 25643725,
       "apps-rollup": 94774012, "apps-webpack": 74482644, "pop-bun": 101194052, "pop-esbuild": 101528412}


def marker_path(line):
    t = line.strip()
    if t.startswith("//#region "):
        p = t[len("//#region "):].strip()
    elif t.startswith("/*!*** ") and t.endswith("***!*/"):
        p = t[len("/*!*** "):-len("***!*/")].strip()
    elif t.startswith("/***/ "):
        r = t[len("/***/ "):].strip()
        if not r.endswith(":"):
            return None
        r = r[:-1].strip()
        if len(r) >= 2 and r[0] == r[-1] and r[0] in "\"'":
            p = r[1:-1]
        else:
            return None
    elif t.startswith("// "):
        r = t[3:].strip()
        if re.search(r"\s", r) or not r.endswith(EXTS):
            return None
        p = r
    else:
        return None
    return p if "node_modules/" in p else None


def marker_package(p):
    rest = p[p.rfind("node_modules/") + len("node_modules/"):]
    s = rest.split("/")
    if s[0].startswith("@"):
        return "/".join(s[:2]) if len(s) > 1 else None
    return s[0] or None


def sponge(pkg, file, path):
    parts = file.split("/")
    if any(p in VENDORED for p in parts[:-1]) or parts[-1].startswith("third-party."):
        return "vendored-path"
    try:
        if os.path.getsize(path) > MAXB:
            return None
        text = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return None
    for line in text.split("\n"):
        mp = marker_path(line)
        if mp and marker_package(mp) != pkg:
            return "bundler-marker"
    return None


def rows_present(want):
    """{(slice, pkg, file)} -> set of versions with any row (k2-all unit tables)."""
    out = collections.defaultdict(set)
    by_slice = collections.defaultdict(set)
    for s, p, f in want:
        by_slice[s].add((p, f))
    for s, keys in by_slice.items():
        pk = {p for p, _ in keys}
        for line in open(f"{IX}/units.{s}.jsonl"):
            i = line.find('"owner":"')
            o = line[i + 9:line.find('"', i + 9)]
            if o not in pk:
                continue
            r = json.loads(line)
            if (r["owner"], r["file"]) in keys:
                for a, b in r["versions"]:
                    out[(s, r["owner"], r["file"])].add((a, b))
    return out


def vkey(v):
    return [int(x) if x.isdigit() else x for x in re.split(r"[.+-]", v)]


def in_range(v, ranges):
    for a, b in ranges:
        try:
            if vkey(a) <= vkey(v) <= vkey(b):
                return True
        except TypeError:
            if a == v or b == v:
                return True
    return False


def main():
    reach, why = {}, {}
    for r in map(json.loads, gzip.open(f"{D}/reach.jsonl.gz", "rt")):
        for f, i in r["files"].items():
            reach[(r["slice"], r["pkg"], r["v"], f)] = i["cls"]
    ne = [json.loads(x) for x in gzip.open(f"{D}/ne-units.jsonl.gz", "rt")]
    keys = {k for k, c in reach.items() if c == "file-emitted"}
    dirs = collections.defaultdict(set)
    for u in ne:
        dirs[(u["slice"], u["pkg"], u["v"], u["file"])].update(u["dirs"])
    for k, c in reach.items():
        if c == "no-own-build":  # the version is in the index only through another root's dependency tree
            root = find_root(k[1], k[2], None, sorted(dirs[k]))
            sp = sponge(k[1], k[3], os.path.join(root, k[3])) if root else None
            why[k] = f"dep-only:sponge:{sp}" if sp else "dep-only:other"
    rp = rows_present({(s, p, f) for s, p, v, f in keys})
    for s, p, v, f in keys:
        work, _ = own_build(p, v, s)
        root = find_root(p, v, work, [])
        sp = sponge(p, f, os.path.join(root, f)) if root else None
        if sp:
            why[(s, p, v, f)] = f"sponge:{sp}"
        elif in_range(v, rp.get((s, p, f), ())):
            why[(s, p, v, f)] = "rows:some"
        else:
            why[(s, p, v, f)] = "rows:none"
    tot = collections.defaultdict(collections.Counter)
    pk = collections.defaultdict(collections.Counter)
    for u in ne:
        k = (u["slice"], u["pkg"], u["v"], u["file"])
        c = reach[k]
        if c in ("file-emitted", "no-own-build"):
            c = why[k]
            if c.endswith("sponge:bundler-marker"):
                c += ":copy" if u["origin"] else ":own"
            pk[u["set"], c][u["pkg"]] += u["b"]
        tot[u["set"]][c] += u["b"]
    out = {}
    for st, c in sorted(tot.items()):
        out[st] = {k: round(100 * v / LIB[st], 3) for k, v in c.most_common()}
        print(st, out[st])
    for (st, c), cc in sorted(pk.items()):
        top = cc.most_common(3)
        tb = sum(cc.values())
        print(f"  {st:13s} {c:34s} pkgs {len(cc):4d}  top share {100 * top[0][1] / tb:5.1f}%  top3 "
              f"{100 * sum(x[1] for x in top) / tb:5.1f}%")
    groups = {"entry-coverage (exports conditions, fields, deep imports)": ("exports:", "field:", "deep:"),
              "own pre-bundled file dropped as a sponge: unit is the package's own code": ("sponge:bundler-marker:own",
                                                                                         "dep-only:sponge:bundler-marker:own"),
              "own pre-bundled file dropped as a sponge: unit is a recognised copy": ("sponge:bundler-marker:copy",
                                                                                      "dep-only:sponge:bundler-marker:copy"),
              "vendored-path sponge": ("sponge:vendored-path", "dep-only:sponge:vendored-path"),
              "file emitted, no row of it (attribution)": ("rows:none",),
              "file emitted, rows of it, not this statement": ("rows:some",),
              "version only a dependency of another root, no sponge": ("dep-only:other",),
              "truth file not in the npm package (chained source)": ("file-not-in-package",)}
    grouped = {st: {g: round(sum(v for k, v in c.items() if k.startswith(pre)), 3) for g, pre in groups.items()}
               for st, c in out.items()}
    here = os.path.dirname(os.path.abspath(__file__))
    json.dump({"unit": "points of the set's library bytes (scorer 9, exp119-k2all-st labels; exp122's not-emitted "
                       "class-d units, decomposed)", "grouped": grouped, "detail": out},
              open(os.path.join(here, "..", "census.json"), "w"), indent=1, sort_keys=True)
    print(json.dumps(grouped, indent=1))


if __name__ == "__main__":
    main()
