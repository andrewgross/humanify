"""exp123 step 1b: which ENTRY of a package reaches the code the index's own build never emitted.

For every not-emitted class-d unit (select_ne.py), by (slice, package, version, truth file):

  1. the package's OWN index build for that slice (recipe/4 work dir `<pkg>@<v>/build/<slice>/{req,dyn}`): absent ->
     `no-own-build` (the version is only in the index as another root's dependency);
  2. the files that build emitted (its source maps). The truth file among them -> `file-emitted` (the file is in
     the build; the unit's statement is not: tree-shaking, or a different part of the file);
  3. otherwise every ENTRY POINT the package declares (a leaf): each leaf of its `exports` map (subpath x the
     condition path down to it), the `main` / `module` / `browser` / `jsnext:main` / `es2015` / `unpkg` /
     `jsdelivr` fields (and the `browser` field's object map). Each leaf's static import closure inside the package
     is taken from an esbuild metafile (platform node, or browser for a `browser` leaf; the leaf's conditions
     active; other packages external). The truth file's class is the cheapest leaf kind whose closure holds it,
     tie broken by the closure's overlap with the files the APP bundled from that package; none -> `deep:*`.

usage: python3 -I reach.py [workers]   -> /work/exp123-data/reach.jsonl.gz (PRIVATE, per key) + aggregates printed
"""
import collections
import gzip
import hashlib
import json
import os
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor

D = "/work/exp123-data"
D122 = "/work/exp123-data/exp122-inputs"
ESBUILD = "/work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild"
WORKS = ["/work/exp113-data/work", "/work/exp109-data/work", "/work/exp108-data/idx/work", "/work/exp101-data/work",
         "/work/exp105-data/work", "/work/exp111-data/work", "/work/exp118-data/idx/work"]
UNITS = ["units-exp119-k2all-st", "units-exp119-k2all-st-apps", "units-pop-exp119-k2all-st"]
NODE_SET = {"node", "import", "require", "default"}
FIELDS = ["main", "module", "browser", "jsnext:main", "es2015", "unpkg", "jsdelivr"]
JS = (".js", ".mjs", ".cjs", ".jsx", ".json")


def own_build(pkg, v, slice_):
    """The recipe/4 build of the package's own root; else (a .recipe pruned) any build of that slice on disk."""
    root = f"{pkg.replace('/', '__')}@{v}"
    fallback = (None, None)
    for w in WORKS:
        d = os.path.join(w, root, "build", slice_)
        r = os.path.join(d, "req", ".recipe")
        if os.path.exists(r) and open(r).readline().strip() == "recipe/4":
            return os.path.join(w, root), d
        if fallback[0] is None and os.path.exists(os.path.join(d, "req", "out.js.map")):
            fallback = (os.path.join(w, root), d)
    return fallback


def emitted(build_dir, pkg):
    """Files of `pkg` (top-level node_modules copy, relative to its root) in the build's maps."""
    out = set()
    marker = f"node_modules/{pkg}/"
    for tag in ("req", "dyn"):
        p = os.path.join(build_dir, tag, "out.js.map")
        if not os.path.exists(p):
            continue
        try:
            srcs = json.load(open(p)).get("sources", [])
        except (OSError, ValueError):
            continue
        for s in srcs:
            i = s.rfind(marker)
            if i >= 0 and "node_modules/" not in s[i + len(marker):]:
                out.add(s[i + len(marker):])
    return out


def find_root(pkg, v, work, dirs):
    """The installed package dir of pkg@v: the own work dir first, then the dep trees of other roots."""
    cands = []
    if work:
        cands.append(os.path.join(work, "node_modules", pkg))
    for d in dirs:
        w = d.split("/build/")[0]
        cands.append(os.path.join(w, "node_modules", pkg))
        nm = os.path.join(w, "node_modules")
        try:
            for x in os.listdir(nm):
                xs = [os.path.join(x, y) for y in os.listdir(os.path.join(nm, x))] if x.startswith("@") else [x]
                cands.extend(os.path.join(nm, y, "node_modules", pkg) for y in xs)
        except OSError:
            pass
    for c in cands:
        try:
            if json.load(open(os.path.join(c, "package.json"))).get("version") == v:
                return c
        except (OSError, ValueError):
            continue
    return None


def export_leaves(exports):
    """(subpath, conditions tuple, target) for every file leaf of an exports map."""
    out = []

    def walk(sub, conds, val):
        if isinstance(val, str):
            if ".d." not in val and not val.endswith(".d.ts"):
                out.append((sub, tuple(conds), val))
        elif isinstance(val, list):
            for x in val:
                walk(sub, conds, x)
        elif isinstance(val, dict):
            for k, x in val.items():
                if k == "types" or k.startswith("types"):
                    continue
                walk(sub, conds + [k], x)

    if isinstance(exports, dict) and any(k.startswith(".") for k in exports):
        for k, x in exports.items():
            if k.startswith("."):
                walk(k, [], x)
    else:
        walk(".", [], exports)
    return out


def norm(t):
    return os.path.normpath(t.lstrip("./") if t.startswith("./") else t).replace("\\", "/")


def leaves(manifest, files):
    """[(kind, label, target file, conditions, platform)]."""
    out = []
    ex = manifest.get("exports")
    if ex is not None:
        for sub, conds, tgt in export_leaves(ex):
            extra = sorted(set(conds) - NODE_SET)
            plat = "browser" if "browser" in conds else "node"
            if "*" in sub or "*" in tgt:
                pre, _, suf = norm(tgt).partition("*")
                for f in files:
                    if f.startswith(pre) and f.endswith(suf) and f.endswith(JS):
                        out.append(("exports", "+".join(extra) or "node-set", f, conds, plat))
                continue
            out.append(("exports", "+".join(extra) or "node-set", norm(tgt), conds, plat))
    for fld in FIELDS:
        val = manifest.get(fld)
        if isinstance(val, str) and val:
            t = norm(val)
            for c in (t, t + ".js", t + "/index.js"):
                if c in files:
                    out.append(("field", fld, c, (), "browser" if fld == "browser" else "node"))
                    break
        elif fld == "browser" and isinstance(val, dict):
            for k, x in val.items():
                if isinstance(x, str) and k.startswith("."):
                    out.append(("field", "browser-map", norm(x), ("browser",), "browser"))
    if ex is None and "main" not in manifest:
        out.append(("field", "main", "index.js", (), "node"))
    return [x for x in out if x[2] in files]


def closure(pkgdir, target, conds, plat, cache_dir):
    key = hashlib.sha1(f"{pkgdir}|{target}|{conds}|{plat}".encode()).hexdigest()
    cp = os.path.join(cache_dir, key + ".json")
    if os.path.exists(cp):
        return set(json.load(open(cp)))
    meta = os.path.join(cache_dir, key + ".meta")
    cmd = [ESBUILD, "./" + target, "--bundle", f"--platform={plat}", "--format=esm", "--packages=external",
           f"--metafile={meta}", "--outfile=/dev/null", "--log-level=silent", "--allow-overwrite",
           "--log-limit=0"]
    c = [x for x in conds if x not in ("import", "require", "default")]
    if c:
        cmd.append("--conditions=" + ",".join(c))
    res = {target}
    try:
        subprocess.run(cmd, cwd=pkgdir, capture_output=True, timeout=120)
        if os.path.exists(meta):
            res |= {norm(p) for p in json.load(open(meta)).get("inputs", {}) if "node_modules/" not in p}
            os.remove(meta)
    except (subprocess.TimeoutExpired, OSError, ValueError):
        pass
    json.dump(sorted(res), open(cp, "w"))
    return res


def package_files(root):
    out = []
    for dp, dn, fn in os.walk(root):
        dn[:] = [d for d in dn if d != "node_modules"]
        for f in fn:
            out.append(os.path.relpath(os.path.join(dp, f), root).replace("\\", "/"))
    return set(out)


RANK = ["exports:node-set", "exports:module", "exports:production", "exports:development", "exports:bun",
        "exports:browser", "exports:other", "field:module", "field:main", "field:jsnext:main", "field:es2015",
        "field:browser", "field:browser-map", "field:unpkg", "field:jsdelivr"]


def leaf_class(kind, label):
    if kind == "field":
        return f"field:{label}"
    if label in ("node-set", "module", "production", "development", "bun", "browser"):
        return f"exports:{label}"
    parts = set(label.split("+"))
    for p in ("browser", "development", "production", "module", "bun"):
        if p in parts:
            return f"exports:{p}"
    return "exports:other"


def work_one(job):
    (slice_, pkg, v), files_wanted, dirs, appfiles = job
    work, bdir = own_build(pkg, v, slice_)
    pdir = find_root(pkg, v, work, dirs)
    res = {"slice": slice_, "pkg": pkg, "v": v, "own": bool(bdir), "files": {}}
    em = emitted(bdir, pkg) if bdir else set()
    if not pdir:
        for f in files_wanted:
            res["files"][f] = {"cls": "no-package-dir"}
        return res
    manifest = json.load(open(os.path.join(pdir, "package.json")))
    pf = package_files(pdir)
    lv = leaves(manifest, pf)
    cache = os.path.join(D, "closures")
    clos = None
    for f in files_wanted:
        info = {"hasExports": "exports" in manifest, "type": manifest.get("type", "commonjs")}
        if f not in pf:
            info["cls"] = "file-not-in-package"
        elif not bdir:
            info["cls"] = "no-own-build"
        elif f in em:
            info["cls"] = "file-emitted"
        else:
            if clos is None:
                clos = [(k, lab, t, closure(pdir, t, list(c), p, cache)) for k, lab, t, c, p in lv]
            hits = []
            for k, lab, t, cl in clos:
                if f in cl:
                    lc = leaf_class(k, lab)
                    if k == "exports" and lab == "node-set" and t in em:
                        lc = "exports:node-set(selected)"
                    hits.append((lc, len(cl & appfiles), t, lab))
            info["hits"] = sorted({(h[0], h[3]) for h in hits})
            if hits:
                hits.sort(key=lambda h: (RANK.index(h[0]) if h[0] in RANK else -1, -h[1]))
                info["cls"] = hits[0][0]
                info["leaf"] = hits[0][2]
            else:
                info["cls"] = "deep:outside-exports" if "exports" in manifest else "deep:no-exports"
            info["emitted"] = len(em)
        res["files"][f] = info
    return res


def main():
    workers = int(sys.argv[1]) if len(sys.argv) > 1 else 16
    os.makedirs(os.path.join(D, "closures"), exist_ok=True)
    ne = [json.loads(x) for x in gzip.open(f"{D}/ne-units.jsonl.gz", "rt")]
    want = collections.defaultdict(set)
    dirs = collections.defaultdict(set)
    ents = collections.defaultdict(set)
    for u in ne:
        k = (u["slice"], u["pkg"], u["v"])
        want[k].add(u["file"])
        dirs[k].update(u["dirs"])
        ents[(u["entry"], u["pkg"])].add(k)
    app = collections.defaultdict(set)  # key -> files the apps bundled from pkg@v
    for name in UNITS:
        for line in gzip.open(f"{D122}/{name}.jsonl.gz", "rt"):
            b = json.loads(line)
            for x in b["units"]:
                if x.get("pkg") and x.get("file") and (b["entry"], x["pkg"]) in ents:
                    k = (b["slice"], x["pkg"], x.get("v"))
                    if k in want:
                        app[k].add(x["file"])
    jobs = [(k, sorted(want[k]), sorted(dirs[k]), app[k]) for k in sorted(want)]
    out = gzip.open(f"{D}/reach.jsonl.gz", "wt")
    with ProcessPoolExecutor(workers) as ex:
        for r in ex.map(work_one, jobs, chunksize=1):
            out.write(json.dumps(r) + "\n")
    out.close()
    cls = {(r["slice"], r["pkg"], r["v"], f): i["cls"] for r in map(json.loads, gzip.open(f"{D}/reach.jsonl.gz", "rt"))
           for f, i in r["files"].items()}
    tot = collections.defaultdict(collections.Counter)
    for u in ne:
        tot[u["set"]][cls[(u["slice"], u["pkg"], u["v"], u["file"])]] += u["b"]
    for s, c in sorted(tot.items()):
        print(s, {k: v for k, v in c.most_common()})


if __name__ == "__main__":
    main()
