"""exp132 scorer 13, rule `misattributed`: the bundled CommonJS modules a source map credits to a file that does
not hold them, relabelled to the file that does (frozen layer `misattr13`).

A source file is ONE module of a bundle. Bun 1.4's maps of large bundles credit whole modules to an unrelated file:
iconv-lite's big5 table to `unpipe`'s index.js, `ms` to vue-router.common.js, css-tree's node modules to
lru-cache's index.min.js. The index builder already drops every file credited to two or more modules
(humanify_index::build, `multi_module_files`). The truth credited them to the wrong package, and a right claim read
as wrong.

Rule (mechanical: the frozen bundle, map and truth, the installed files, and the module spans of one reference
run; no claim, no index). A factory F (a `Factory` unit row of the reference run, bundler runtime excluded) whose
majority-mapped source S of package P
  1. is the majority source of at least one OTHER factory of the bundle (one file, two modules: the builder's
     signal), and
  2. some OTHER factory credited to S holds S's tokens (S's own module is elsewhere in the bundle), and
  3. S does not hold F's code (and P's installed files do not hold F's long strings): of F's RARE tokens (truth_audit.TOKEN, held by 1..max(5, 2% of) the map's sources),
     at least MIN_TOKENS, less than OWN of them are in any source of P; or, for a data module (MIN_STRINGS long
     string literals without escapes, a JSON table: Bun maps no JSON), less than OWN of its long strings are in
     any of P's sources nor of P's installed files,
takes the SMALLEST file of ANOTHER package, no bigger than SIZE_FACTOR x F + SIZE_SLACK (one file is one module: a
whole bundled dist holding F's code is a copy, not F's file), holding at least max(FOUND, own + OWN) of them: first among the map's
sources (sourcesContent), else among the installed files of the packages the map names (`installed`). No such
file: F keeps its label (the rule relabels only to a file shown to hold the code).

At score time a unit whose module is a layer row's factory, and whose majority-mapped source is still the row's old
file S (a body statement mapped to another file of P keeps its label), takes
the row's package, version and file (truth_audit.Audit.misattributed); its copy origin is then read for the new
shipper.

usage: <venv python> -I misattr13.py build --corpus C --units <label>
       writes <DATA>/truth/<id>.misattr13.json.gz per sourcemap entry and misattr13[-<corpus>].json beside copies.json;
       prints aggregates per set only (no names)."""
import collections
import gzip
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.normpath(os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, SCORE)
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402

SCHEMA = "exp132-misattr13/4"
MIN_TOKENS = 5
MIN_STRINGS = 3
OWN = 0.5
FOUND = 0.8
RARE_MIN = 5
RARE_SHARE = 0.02
STRING = re.compile(rb'"([^"\\\n]{20,})"|\'([^\'\\\n]{20,})\'')
EXTS = (".js", ".cjs", ".mjs", ".json")
MAX_FILE = 16 << 20
SIZE_FACTOR = 8
SIZE_SLACK = 8192


def manifest_path(corpus=None):
    c = corpus or lib096.CORPUS
    return os.path.join(SCORE, "misattr13.json" if c == "dev" else f"misattr13-{c}.json")


def package_root(path):
    """(package name, root dir) of a path under node_modules, else None."""
    if "/node_modules/" not in path:
        return None
    head, rest = path.rsplit("/node_modules/", 1)
    parts = rest.split("/")
    n = 2 if parts[0].startswith("@") and len(parts) > 1 else 1
    return "/".join(parts[:n]), os.path.join(head, "node_modules", *parts[:n])


class Installed:
    """The installed files of the packages a bundle's map names, read once (lazily) per entry."""

    def __init__(self, base, srcs):
        self.roots = {}
        for s in srcs:
            if isinstance(s, str):
                r = package_root(os.path.normpath(os.path.join(base, s)))
                if r and os.path.isdir(r[1]):
                    self.roots[r[1]] = r[0]
        self._files = None

    def files(self):
        if self._files is None:
            self._files = []
            for root, pkg in sorted(self.roots.items()):
                for d, dirs, names in os.walk(root):
                    dirs[:] = sorted(x for x in dirs if x != "node_modules")
                    for n in sorted(names):
                        p = os.path.join(d, n)
                        if n.endswith(EXTS) and not os.path.islink(p):
                            try:
                                if os.path.getsize(p) <= MAX_FILE:
                                    self._files.append((pkg, root, p))
                            except OSError:
                                pass
        return self._files

    def holders(self, needles, need, not_pkg=None, only_pkg=None):
        out = []
        for pkg, root, p in self.files():
            if pkg == not_pkg or (only_pkg is not None and pkg != only_pkg):
                continue
            try:
                b = open(p, "rb").read()
            except OSError:
                continue
            if sum(1 for x in needles if x in b) >= need:
                out.append((len(b), p, pkg, root))
        return sorted(out)


def module_sized(file_bytes, module_bytes):
    """Can a file of this size be ONE module of this size? A source is one file and a CommonJS module one file:
    a file far bigger than the module (a package's whole bundled dist, a vendored copy of many files) holds the
    module's code as a copy, it is not the module's file."""
    return file_bytes <= SIZE_FACTOR * module_bytes + SIZE_SLACK


def version_of(root):
    try:
        return json.load(open(os.path.join(root, "package.json"))).get("version")
    except (OSError, ValueError):
        return None


def factories(units):
    return [u for u in units if u["k"] == "Factory" and not u["rt"]]


def majority(segs, offs, s, e):
    import bisect
    k = max(bisect.bisect_right(offs, s) - 1, 0)
    votes = collections.Counter()
    while k < len(segs) and segs[k][0] < e:
        a, b = max(segs[k][0], s), min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
        if b > a:
            votes[segs[k][1]] += b - a
        k += 1
    return sorted(votes.items(), key=lambda kv: (-kv[1], kv[0]))[0][0] if votes else None


def entry_layer(entry, truth, units, agg):
    srcs = truth["sources"]
    mp = lib096.corpus_path(entry, "innerMap" if "innerMap" in entry["files"] else "map")
    smap = json.load(open(mp))
    contents = smap.get("sourcesContent") or []
    tok = {si: set(truth_audit.TOKEN.findall(c)) for si, c in enumerate(contents) if c}
    raw = {si: c.encode() for si, c in enumerate(contents) if c}
    df = collections.Counter()
    for t in tok.values():
        df.update(t)
    common = max(RARE_MIN, RARE_SHARE * len(tok))
    text = open(lib096.corpus_path(entry, "bundle"), "rb").read()
    segs, offs = truth["segments"], [s[0] for s in truth["segments"]]
    major = {}
    for u in factories(units):
        si = majority(segs, offs, u["s"], u["e"])
        if si is not None:
            major[(u["s"], u["e"])] = si
    credited = collections.Counter(major.values())
    sharing = collections.defaultdict(list)
    for span, si in major.items():
        sharing[si].append(span)
    base = os.path.dirname(entry["files"]["bundle"]["origin"])
    installed = Installed(base, smap.get("sources") or [])
    by_pkg_tok, by_pkg_raw = {}, {}
    rows = []
    for (s, e), si in sorted(major.items()):
        pkg = srcs[si][0] if si < len(srcs) else "<unmapped>"
        if credited[si] < 2 or pkg.startswith("<"):
            continue
        agg["factories sharing a source"] += 1
        body = text[s:e]
        if si not in tok:
            continue

        def matches(span, si=si):
            tt = {x for x in truth_audit.TOKEN.findall(text[span[0]:span[1]].decode("utf8", "replace"))
                  if 0 < df[x] <= common}
            return len(tt) >= MIN_TOKENS and len(tt & tok[si]) >= OWN * len(tt)
        # S's own module is elsewhere in the bundle: another factory credited to S holds S's tokens
        if not any(matches(sp) for sp in sharing[si] if sp != (s, e)):
            agg["no sibling module holds the shared file (kept)"] += 1
            continue
        if pkg not in by_pkg_tok:
            mine = [sj for sj in tok if sj < len(srcs) and srcs[sj][0] == pkg]
            by_pkg_tok[pkg] = set().union(*(tok[sj] for sj in mine)) if mine else set()
            by_pkg_raw[pkg] = [raw[sj] for sj in mine]
        t = {x for x in truth_audit.TOKEN.findall(body.decode("utf8", "replace")) if 0 < df[x] <= common}
        new = None
        strs = sorted({m.group(1) or m.group(2) for m in STRING.finditer(body)})
        if len(strs) >= MIN_STRINGS and installed.holders(strs, OWN * len(strs), only_pkg=pkg):
            agg["strings in the labelled package's installed files (kept)"] += 1
            continue
        if len(t) >= MIN_TOKENS:
            own = len(t & by_pkg_tok[pkg]) / len(t)
            if own < OWN:
                need = max(FOUND, own + OWN) * len(t)
                held = [sj for sj, tj in tok.items() if sj < len(srcs) and not srcs[sj][0].startswith("<")
                        and srcs[sj][0] != pkg and len(t & tj) >= need and module_sized(len(raw[sj]), e - s)]
                if held:
                    sj = min(held, key=lambda x: (len(raw[x]), x))
                    new = [srcs[sj][0], srcs[sj][1], srcs[sj][2], "map"]
                else:
                    agg["tokens absent, no map source holds them"] += 1
        if new is None:
            if len(strs) >= MIN_STRINGS:
                own = sum(1 for x in strs if any(x in r for r in by_pkg_raw[pkg])) / len(strs)
                if own < OWN:
                    need = max(FOUND, own + OWN) * len(strs)
                    held = [h for h in installed.holders(strs, need, pkg) if module_sized(h[0], e - s)]
                    if held:
                        _, p, hp, root = held[0]
                        new = [hp, version_of(root), os.path.relpath(p, root), "installed"]
                    else:
                        agg["strings absent, no installed file holds them"] += 1
        if new is None:
            continue
        agg["relabelled (" + new[3] + ")"] += 1
        agg["relabelled bytes (" + new[3] + ")"] += e - s
        rows.append([s, e, pkg] + new + [srcs[si][2]])
    return rows


def build(units_label):
    chm = truth_audit.load_chained_manifest()
    om, o7m = lib096.load_origins_manifest(), truth_audit.load_origins7_manifest()
    entries = [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    shas = {}
    agg = collections.defaultdict(collections.Counter)
    for e in entries:
        p = os.path.join(lib096.DATA, "results", units_label, "reports", e["id"] + ".json")
        rep = json.load(open(p))["libraryDiscovery"]
        units = score.run_units(rep) if rep else []
        truth = truth_audit.patch_sources(e, lib096.load_truth(e), chm)
        truth, _ = truth_audit.redecode(e, truth, truth_audit.origin_layer(e, om, o7m))
        rows = entry_layer(e, truth, units, agg[e["set"]])
        out = os.path.join(lib096.DATA, "truth", e["id"] + ".misattr13.json.gz")
        with gzip.GzipFile(out, "wb", mtime=0) as fh:
            fh.write(lib096.dump({"schema": SCHEMA, "rows": rows}).encode())
        shas[e["id"]] = lib096.sha256_file(out)
    man = {"schema": SCHEMA, "corpusSha256": lib096.manifest_sha(), "unitsLabel": units_label, "entries": shas,
           "rules": {"minTokens": MIN_TOKENS, "minStrings": MIN_STRINGS, "own": OWN, "found": FOUND,
                     "rareMin": RARE_MIN, "rareShare": RARE_SHARE, "sizeFactor": SIZE_FACTOR,
                     "sizeSlack": SIZE_SLACK},
           "about": "scorer 13 rule `misattributed` (experiments/132-factory-precision/scorer13/misattr13.py): per "
                    "entry, the sha256 of <DATA>/truth/<id>.misattr13.json.gz (rows: factory start, end, old "
                    "package, new package, version, file, evidence, old file)"}
    open(manifest_path(), "w").write(lib096.dump(man))
    for s, c in sorted(agg.items()):
        for k, v in sorted(c.items()):
            print(f"{s:16s} {v:>12,}  {k}")


if __name__ == "__main__":
    argv = lib096.corpus_flag(sys.argv)
    if argv[1:2] != ["build"] or "--units" not in argv:
        raise SystemExit(__doc__)
    build(argv[argv.index("--units") + 1])
