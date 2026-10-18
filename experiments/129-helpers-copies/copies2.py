"""exp129: the COPIES layer, version 2: derived from the INSTALLED packages and the index's evidence over every
unit of a corpus, not from one reference label's run.

usage: <venv python> -I copies2.py hashes <out.pkl> <corpus>=<units label>[,<units label>...] ...
       <venv python> -I copies2.py rows <index dir>... <hashes.pkl> <out.pkl>
       <venv python> -I copies2.py derive <rows.pkl> <units label>[,<units label>...] --corpus C [--jobs N]
writes <DATA>/truth/<id>.copies2.json.gz per source-mapped entry and experiments/096-library-matching-score/
       copies2.json (copies2-<corpus>.json for the others): their sha256s, the index, the rule's constants.

exp104's copies rule (experiments/104-truth-audit/copies.py), unchanged in what it asks; what changed is where each
test reads its evidence:
  code       the prints of O's family in the index cover >= COVER of the unit (its whole print, else O-family
             function prints, largest non-overlapping first, each >= MIN_NODES nodes); a print held by more than
             MAX_FAMILIES families is shared helper code, not evidence. Rows from EVERY slice of the index (exp104
             read one slice of the reference label's index: a terser-minified bundle's inlined copy matches only
             the terser-built slices, exp127). O's family is not the shipper S's.
  dependency the INSTALLED S@v's own package.json (node_modules/<S>/package.json at the path the source map names,
             its version equal to the truth's) declares O or a package linked to it (dependencies, dev, peer,
             optional, bundled). exp104 read a registry cache that lacked many versions (870 KB `noManifest` on
             apps); the cache is now the fallback only.
  named      the shipped file the unit maps into names O or the declaring dependency as a word (the map's
             sourcesContent, else the installed file) — a module specifier naming it does not count (an import
             makes the file O's consumer; exp104's test passed Angular boilerplate in a file importing
             @angular/common).
  unique     exactly one family passes (the largest cover; a tie is no copy).
  mapped     the unit has a source-map segment of its own.
UNITS: every judged unit of the corpus (statement units and lazy bodies, and factory body statements when the run
cut factories) with prints. A run's unitRows are the unit model's (the same for every label of one binary: no
claim, veto or setting is read); the layer is frozen as byte ranges, so score.py reads it for any label's units.
"""
import bisect
import collections
import json
import multiprocessing
import os
import pickle
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.join(HERE, "../096-library-matching-score")
sys.path.insert(0, SCORE)
sys.path.insert(0, os.path.join(HERE, "../104-truth-audit"))
import lib096  # noqa: E402
import mmlib  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402
from families_lib import base_family  # noqa: E402
from lib096 import dump, sha256_file, write_gz_json  # noqa: E402

SCHEMA = "exp129-copies/2"
COVER = 0.5
MIN_NODES = 30
MAX_FAMILIES = 3
# the manifest caches the frozen exp104 layers read (<pkg>@<version>.json): the fallback when no install resolves
REGISTRIES = ("/work/exp104-data/registry", "/work/exp107-data/registry", "/work/exp112-data/registry",
              "/work/exp116-test2-data/registry")
DEP_KEYS = ("dependencies", "devDependencies", "peerDependencies", "optionalDependencies", "bundleDependencies",
            "bundledDependencies")
CTX = {}


def judged_units(units):
    """The units the scorer judges that carry prints: statement units and lazy bodies, plus factory body
    statements (exp128's FactoryBody rows) when the run cut factories."""
    return [u for u in units if u["k"] in ("Statement", "LazyBody", "FactoryBody") and not u["rt"] and "w" in u]


def print_hashes(u):
    whole, fns = mmlib.prints(u)
    out = [whole[2]] if whole[3] >= MIN_NODES else []
    return out + [f[2] for f in fns if f[3] >= MIN_NODES]


# ---------------------------------------------------------------- step 1: every print hash the corpora hold

def entry_hashes(args):
    corpus, labels, entry = args
    lib096.use(corpus)
    out = set()
    for label in labels.split(","):
        rp = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
        rep = json.load(open(rp))["libraryDiscovery"] if os.path.exists(rp) else None
        if rep is not None:
            out |= {h for u in judged_units(score.run_units(rep)) for h in print_hashes(u)}
    return out


def cmd_hashes(argv):
    out, pairs = argv[0], [a.split("=", 1) for a in argv[1:]]
    todo = []
    for corpus, label in pairs:
        lib096.use(corpus)
        todo += [(corpus, label, e) for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    hs = set()
    with multiprocessing.Pool(12) as pool:
        for s in pool.imap_unordered(entry_hashes, todo, chunksize=1):
            hs |= s
    pickle.dump({"units": dict(pairs), "hashes": hs}, open(out, "wb"), protocol=5)
    print(len(hs), "hashes")


# ---------------------------------------------------------------- step 2: the index rows holding them, every slice

def slice_rows(args):
    path, hashes_path = args
    hs = pickle.load(open(hashes_path, "rb"))["hashes"]
    rows = collections.defaultdict(set)
    with open(path) as fh:
        for line in fh:
            h = line[9:25]  # {"hash":"<16 hex>", ... — the merge writes `hash` first
            if h in hs:
                rows[h].add(json.loads(line)["owner"])
    return path, dict(rows)


def helper_rows(ix, hs):
    """The index's helper table (exp129: the helper sources' own prints), as one more table of rows."""
    rows = collections.defaultdict(set)
    hp = os.path.join(ix, "helpers.jsonl")
    if os.path.exists(hp):
        for line in open(hp):
            r = json.loads(line)
            if r["hash"] in hs:
                rows[r["hash"]] |= set(r["owners"])
    return dict(rows)


def cmd_rows(argv):
    """rows <index dir>... <hashes.pkl> <out.pkl>: one row TABLE per (index, slice), plus each index's helper
    table — evidence is judged per table (the commonness bound is a property of one table's population)."""
    *ixs, hashes_path, out = argv
    hs = pickle.load(open(hashes_path, "rb"))["hashes"]
    todo = [(os.path.join(ix, f), hashes_path) for ix in ixs for f in sorted(os.listdir(ix))
            if f.startswith("units.") and f.endswith(".jsonl")]
    tables = {}
    with multiprocessing.Pool(min(len(todo), 12)) as pool:
        for path, r in pool.imap_unordered(slice_rows, todo):
            tables[path] = r
    indexes = {}
    for ix in ixs:
        h = helper_rows(ix, hs)
        if h:
            tables[os.path.join(ix, "helpers.jsonl")] = h
        indexes[ix] = lib096.sha256_bytes(open(os.path.join(ix, "manifest.json"), "rb").read())
    pickle.dump({"indexes": indexes, "tables": dict(sorted(tables.items()))}, open(out, "wb"), protocol=5)
    for t, r in sorted(tables.items()):
        print(t, len(r), "hashes held")


# ---------------------------------------------------------------- step 3: the layer

def install_root(source_path, pkg):
    """node_modules/<pkg> on the source's path (the LAST such segment: a nested install), or None."""
    marker = "/node_modules/" + pkg + "/"
    k = source_path.rfind(marker)
    return source_path[:k + len(marker) - 1] if k >= 0 else None


def read_deps(d):
    out = set()
    for k in DEP_KEYS:
        v = d.get(k) or {}
        out.update(v if isinstance(v, list) else v.keys())
    return out


def manifest_deps(root, pkg, ver):
    """(declared dependencies, where read): the installed package.json when its version is the truth's, else
    the manifest caches; (None, why) when none has it."""
    if root is not None:
        p = os.path.join(root, "package.json")
        if os.path.exists(p):
            try:
                d = json.load(open(p))
            except ValueError:
                d = {}
            if d.get("name") == pkg and str(d.get("version")) == str(ver):
                return read_deps(d), "installed"
    for reg in REGISTRIES:
        p = os.path.join(reg, pkg.replace("/", "__") + "@" + str(ver) + ".json")
        if os.path.exists(p):
            d = json.load(open(p))
            if "__error" not in d:
                return read_deps(d), "registry"
    return None, "none"


def named(text, name):
    """The shipped text names `name` as a word, NOT counting a module specifier (`from "name"`, `require("name")`,
    `import("name")`, `import "name"`, `export ... from "name"`, subpaths included): a file importing a package
    is its consumer, the package's code is not in it (Angular's ɵfac boilerplate in @ng-bootstrap's fesm, which
    imports @angular/common, matched @angular/common's rows)."""
    q = re.escape(name)
    # `from` / `import` take a string literal; require() and import() also a template literal
    static = r"(?:\bfrom\s*|\bimport\s+)(['\"])" + q + r"(?:/[^'\"`]*)?\1"
    call = r"(?:\brequire\s*\(\s*|\bimport\s*\(\s*)(['\"`])" + q + r"(?:/[^'\"`]*)?\1"
    text = re.sub(call, " ", re.sub(static, " ", text))
    return re.search(r"(?<![\w-])" + q + r"(?![\w-])", text) is not None


def declared(dep, origin):
    """exp104's declared(): the same package, a linked family (families.json or derived), or one base family
    that is not merely one npm scope."""
    if dep == origin or CTX["fams"].linked(dep, origin):
        return True
    a, b = base_family(dep), base_family(origin)
    return a == b and not a.startswith("scope:")


def owner_cover(u, same):
    """{owner: nodes its family's rows cover}, the best over the row tables (one per index slice, and each
    index's helper table): exp104's owner_cover read in each table, the commonness bound in that table."""
    whole, fns = mmlib.prints(u)
    best = {}
    for rows in CTX["tables"]:
        def owners(h, rows=rows):
            o = rows.get(h, set())
            return o if len({base_family(x) for x in o}) <= MAX_FAMILIES else set()
        whole_owners = owners(whole[2]) if whole[3] >= MIN_NODES else set()
        fn_owners = [(f, owners(f[2])) for f in fns if f[3] >= MIN_NODES]
        cands = whole_owners.union(*(o for _, o in fn_owners))
        for o in cands:
            if any(same(o, w) for w in whole_owners):
                n = whole[3]
            else:
                n = mmlib.greedy_cover([f for f, os_ in fn_owners if any(same(o, x) for x in os_)])
            best[o] = max(best.get(o, 0), n)
    return best


def source_paths(entry, m):
    base = os.path.dirname(entry["files"]["map"]["origin"])
    root = m.get("sourceRoot") or ""
    return [os.path.normpath(os.path.join(base, root, s)) for s in m.get("sources", [])]


def shipped_text(contents, paths, si):
    if si < len(contents) and contents[si]:
        return contents[si]
    if si < len(paths) and os.path.isfile(paths[si]):
        return open(paths[si], errors="replace").read()
    return ""


def entry_units(entry):
    """The judged units of every units label's run of `entry` (labels comma-separated: a default run and a
    run that cut factories into body statements see different units of one bundle), once per span, renumbered;
    None when no run discovered anything."""
    out, seen = [], set()
    for label in CTX["label"].split(","):
        p = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
        if not os.path.exists(p):
            continue
        rep = json.load(open(p))["libraryDiscovery"]
        if rep is None:
            continue
        for u in judged_units(score.run_units(rep)):
            if (u["s"], u["e"]) not in seen:
                seen.add((u["s"], u["e"]))
                out.append(dict(u, i=len(out)))
    return out or None


def entry_copies(entry):
    same = CTX["same"]
    judged = entry_units(entry)
    rows, stats = [], collections.Counter()
    if judged is not None:
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), CTX["chm"])
        origins = truth_audit.origin_layer(entry, CTX["om"], CTX["o7m"])
        truth, origins = truth_audit.redecode(entry, truth, origins)
        segs = truth["segments"]
        offs = [s[0] for s in segs]
        mp = lib096.corpus_path(entry, "innerMap" if "innerMap" in entry["files"] else "map")
        m = json.load(open(mp))
        contents = m.get("sourcesContent") or []
        paths = source_paths(entry, m) if "innerMap" not in entry["files"] else []
        labels = label_units(entry, truth, judged, judged, origins)
        for u in judged:
            lab = labels.get(u["i"])
            if lab is None or lab["label"] in score.NOT_LIBRARY or "origin" in lab:
                continue
            if bisect.bisect_left(offs, u["s"]) == bisect.bisect_left(offs, u["e"]):
                continue  # carried: not this rule's question
            cov = {o: n for o, n in owner_cover(u, same).items()
                   if not same(o, lab["label"]) and n >= COVER * max(u["w"][1], 1)}
            if not cov:
                continue
            stats["codeMatch"] += u["b"]
            si = segs[bisect.bisect_right(offs, u["s"]) - 1][1]
            root = install_root(paths[si], lab["pkg"]) if si < len(paths) else None
            deps, where = manifest_deps(root, lab["pkg"], lab.get("version"))
            if deps is None:
                stats["noManifest"] += u["b"]
                continue
            stats["manifest:" + where] += u["b"]
            ok = {o: (n, sorted(d for d in deps if declared(d, o))) for o, n in cov.items()}
            ok = {o: v for o, v in ok.items() if v[1]}
            if not ok:
                stats["notADependency"] += u["b"]
                continue
            text = shipped_text(contents, paths, si)
            ok = {o: v for o, v in ok.items() if any(named(text, x) for x in [o, *v[1]])}
            if not ok:
                stats["notNamed"] += u["b"]
                continue
            best = max(n for n, _ in ok.values())
            top = sorted(o for o, (n, _) in ok.items() if n == best)
            if any(not same(top[0], o) for o in top[1:]):
                stats["tie"] += u["b"]
                continue
            stats["copy"] += u["b"]
            rows.append([u["s"], u["e"], lab["label"], top[0], ok[top[0]][1][0],
                         round(best / max(u["w"][1], 1), 4)])
    p = os.path.join(lib096.DATA, "truth", entry["id"] + ".copies2.json.gz")
    write_gz_json(p, {"schema": SCHEMA, "ranges": sorted(rows), "stats": dict(stats)})
    return entry["id"], sha256_file(p), dict(stats)


def label_units(entry, truth, units, judged, origins):
    """score.label_units over the judged units (statement scope, and factory bodies when present) with the
    carried rule and NO copies layer (the layer this builds): truth_audit.Audit with no copy ranges."""
    audit = truth_audit.Audit(entry, truth, None, ranges=[])
    keep = {u["i"] for u in judged}
    subset = [dict(u, k="Statement") if u["k"] == "FactoryBody" else u for u in units if u["i"] in keep]
    return score.label_units(truth, subset, origins, audit)


def init(label, rows_path, corpus):
    lib096.use(corpus)
    import families_lib
    fams = families_lib.Families()
    r = pickle.load(open(rows_path, "rb"))
    CTX.update(label=label, same=fams.same, fams=fams, tables=list(r["tables"].values()),
               om=lib096.load_origins_manifest(),
               o7m=truth_audit.load_origins7_manifest(), chm=truth_audit.load_chained_manifest())


def copies2_path(corpus):
    return os.path.join(SCORE, "copies2.json" if corpus == "dev" else f"copies2-{corpus}.json")


def cmd_derive(argv):
    import families_lib
    argv = lib096.corpus_flag(argv)
    rows_path, label = argv[0], argv[1]
    jobs = int(argv[argv.index("--jobs") + 1]) if "--jobs" in argv else 12
    todo = [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    with multiprocessing.Pool(jobs, initializer=init, initargs=(label, rows_path, lib096.CORPUS)) as pool:
        res = pool.map(entry_copies, todo, chunksize=1)
    r = pickle.load(open(rows_path, "rb"))
    tot = collections.Counter()
    for _, _, s in res:
        tot.update(s)
    open(copies2_path(lib096.CORPUS), "w").write(dump({
        "schema": SCHEMA, "corpusSha256": lib096.manifest_sha(), "unitsFrom": label,
        "indexes": {os.path.basename(k): v for k, v in sorted(r["indexes"].items())},
        "tables": [os.path.relpath(t, os.path.dirname(os.path.dirname(t))) for t in r["tables"]],
        "cover": COVER, "minNodes": MIN_NODES,
        "maxFamilies": MAX_FAMILIES, "familiesSha256": families_lib.derived_sha(),
        "bytes": dict(sorted(tot.items())), "entries": {k: sha for k, sha, _ in sorted(res)}}))
    print(lib096.CORPUS, dict(sorted(tot.items())))


def main():
    cmd, argv = sys.argv[1], sys.argv[2:]
    {"hashes": cmd_hashes, "rows": cmd_rows, "derive": cmd_derive}[cmd](argv)


if __name__ == "__main__":
    main()
