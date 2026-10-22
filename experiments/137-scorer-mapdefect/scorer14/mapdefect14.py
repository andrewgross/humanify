"""exp137 scorer 14, rule `mapdefect`: a unit whose source-map label names a package whose INSTALLED files (and its
installed dependencies) do not hold the unit's code, while exactly one other installed package's file does, is
that package's (frozen layer `mapdefect14`).

Bun 1.4's maps credit code to the wrong file (exp132 found it for whole modules sharing a file; exp136 for single
units: a 42 KB encoding table in three apps labelled another package's index.js). Scorer 13's rule covers only a
module whose file is ALSO another module's majority source; this one reads the unit's own evidence against the
installed packages, whatever the map's shape.

Evidence, independent of the matcher (no index row, no claim, no score is read): the frozen bundle, map and truth,
the app's OWN install tree (the node_modules the map's sources live in), and the unit spans of the reference runs
(which units exist; the unit model, the same for every label of one binary).

Rule, per judged unit U (statement, lazy body, factory, factory body; bundler runtime excluded) whose scorer-13
label is a library package T with no copy origin and no scorer 13 relabel:
  evidence    U's NAME EVIDENCE (tokens14.mjs, acorn: string / template values, `.property` names, object keys of 4+
              characters; `clean`: verbatim-searchable, less a stoplist of names every file holds), kept when
              DISTINCTIVE: held by at most DF_MAX installed packages of the app (by name; a token in more than
              FILE_CAP files is not). At least MIN_EVIDENCE distinctive tokens, else the label stands. More than
              SAMPLE: the SAMPLE first by sha1 (a fixed, content-blind draw).
  (a) lacks   T's installed CLOSURE — T's install root on the map's source path, and every dependency /
              optional / peer dependency resolved from it the way node does (nested, hoisted, pnpm siblings),
              transitively, all their files together — holds under LACK of the tokens (substring).
  (b) holds   a SINGLE installed file of a package outside that closure holds at least HOLD of them (substring).
  unique      every such file belongs to one family (families_lib), which is not T's (another version of T holding
              it means T's family holds it) and not the app's own package.
Then U takes that package (the file holding the most; version from its package.json). Anything else — T not
installed at the path the map names, too little evidence, T's closure holding LACK or more, no holder, two families —
leaves the label as it is. The rule does not know who claimed U: it moves a claim right -> wrong as readily as
wrong -> right, and it relabels unclaimed units (recall's truth) the same way.

At score time (truth_audit.Audit.mapdefect) a unit whose span is a layer row's and whose label is still the row's
old package takes the row's package, version and file; its copy origin is then read for the new shipper.

usage: <venv python> -I mapdefect14.py build --corpus C --units <label>[,<label>...] [--jobs N]
       writes <DATA>/truth/<id>.mapdefect14.json.gz per source-mapped entry and misattr-style
       mapdefect14[-<corpus>].json beside copies.json; prints aggregates per set only (no names)."""
import collections
import gzip
import hashlib
import json
import multiprocessing
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.normpath(os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, SCORE)
import lib096  # noqa: E402

SCHEMA = "exp137-mapdefect14/1"
MIN_EVIDENCE = 8
DF_MAX = 5
LACK = 0.2
HOLD = 0.8
CANDIDATE = 0.3  # token-set share that makes a file a candidate for the substring check
SAMPLE = 64
FILE_CAP = 1000
EXTS = (".js", ".cjs", ".mjs", ".json")
MAX_FILE = 16 << 20
DEP_KEYS = ("dependencies", "optionalDependencies", "peerDependencies")
ACORN = "/work/exp105-data/tools/node_modules"
TOKENS = os.path.join(HERE, "tokens14.mjs")
# property / string names nearly every file holds: no evidence of WHICH file a unit came from (exp136 dcheck.py)
COMMON = set("""length prototype call apply bind push pop shift unshift slice splice concat join split indexOf
includes forEach map filter reduce some every keys values entries assign create freeze defineProperty
getOwnPropertyNames getOwnPropertyDescriptor hasOwnProperty toString valueOf constructor name type value
data default exports module then catch finally resolve reject done next return throw charCodeAt charAt
substring substr replace match test exec trim toLowerCase toUpperCase startsWith endsWith isArray
string number object function boolean undefined symbol bigint message stack error code options args
start end size from emit listeners parent node props state self window document process env""".split())


def manifest_path(corpus=None):
    c = corpus or lib096.CORPUS
    return os.path.join(SCORE, "mapdefect14.json" if c == "dev" else f"mapdefect14-{c}.json")


def clean(toks):
    """The tokens that can be looked up verbatim in a source file: 4..200 characters with three letters in a row,
    no character a source would have to escape (quote, backslash, control), less COMMON (exp136's clean)."""
    import re
    return {t for t in toks if 4 <= len(t) <= 200 and re.search(r"[A-Za-z]{3}", t)
            and not any(c in t for c in "\"'`\\") and all(" " <= c for c in t)} - COMMON


def node(req):
    r = subprocess.run(["node", "--max-old-space-size=8192", TOKENS], input=json.dumps(req), capture_output=True,
                       text=True, env={**os.environ, "NODE_PATH": ACORN})
    if r.returncode != 0:
        raise SystemExit(f"tokens14.mjs failed: {r.stderr[-400:]}")
    return json.loads(r.stdout)


# ---------------------------------------------------------------- the install tree

def package_of(path):
    """(package name, root dir) of a path under node_modules (its innermost node_modules segment), else None."""
    if "/node_modules/" not in path:
        return None
    head, rest = path.rsplit("/node_modules/", 1)
    parts = rest.split("/")
    if not parts[0] or parts[0].startswith("."):
        return None
    n = 2 if parts[0].startswith("@") and len(parts) > 1 else 1
    return "/".join(parts[:n]), os.path.join(head, "node_modules", *parts[:n])


def tree_files(tops):
    """[(package, real root, real path)] of every .js/.cjs/.mjs/.json file (at most MAX_FILE bytes) under the
    given node_modules dirs, symlinks followed, each real file once."""
    seen_dirs, seen_files, out = set(), set(), []
    for top in tops:
        for d, dirs, names in os.walk(top, followlinks=True):
            rd = os.path.realpath(d)
            if rd in seen_dirs:
                dirs[:] = []
                continue
            seen_dirs.add(rd)
            dirs.sort()
            for n in sorted(names):
                if not n.endswith(EXTS):
                    continue
                p = os.path.realpath(os.path.join(d, n))
                if p in seen_files or not os.path.isfile(p):
                    continue
                seen_files.add(p)
                po = package_of(p)
                try:
                    if po is None or os.path.getsize(p) > MAX_FILE:
                        continue
                except OSError:
                    continue
                out.append((po[0], po[1], p))
    return out


def read_json(p):
    try:
        with open(p) as fh:
            d = json.load(fh)
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def resolve(dep, from_root, stop):
    """The real root of `dep` as node resolves it from the package at from_root: <dir>/node_modules/<dep> for the
    package's own dir and each ancestor up to `stop` (the app root), on the real path and on the path as given."""
    out = []
    for start in dict.fromkeys([from_root, os.path.realpath(from_root)]):
        d = start
        while True:
            c = os.path.join(d, "node_modules", dep)
            if os.path.isfile(os.path.join(c, "package.json")):
                out.append(os.path.realpath(c))
                break
            if d == stop or os.path.dirname(d) == d or not d.startswith(stop):
                break
            d = os.path.dirname(d)
    return out


def closure(root, stop):
    """The real roots of the package installed at `root` and of every dependency, optional and peer dependency
    resolved from it, transitively (the installed code the package can run)."""
    todo, done = [os.path.realpath(root)], set()
    while todo:
        r = todo.pop()
        if r in done:
            continue
        done.add(r)
        pj = read_json(os.path.join(r, "package.json"))
        for k in DEP_KEYS:
            v = pj.get(k) or {}
            for dep in sorted(v if isinstance(v, (list, dict)) else []):
                if isinstance(dep, str):
                    todo.extend(resolve(dep, r, stop))
    return done


def version_of(root):
    v = read_json(os.path.join(root, "package.json")).get("version")
    return str(v) if v is not None else None


# ---------------------------------------------------------------- the rule

def distinctive(tokens, df, many):
    """The unit's distinctive tokens: held by at most DF_MAX installed packages (by name) and in at most FILE_CAP
    files; a token no installed file holds is kept (it is evidence nobody holds)."""
    return sorted(t for t in tokens if t not in many and df.get(t, 0) <= DF_MAX)


def sample(tokens):
    """At most SAMPLE tokens, the first by sha1 (content-blind, deterministic)."""
    if len(tokens) <= SAMPLE:
        return list(tokens)
    return sorted(tokens, key=lambda t: hashlib.sha1(t.encode()).hexdigest())[:SAMPLE]


def share_in(tokens, text):
    return sum(1 for t in tokens if t in text) / len(tokens) if tokens else 0.0


def decide(label, evidence, closure_share, holders, same_family, app_pkg):
    """The rule's verdict for one unit labelled `label`. evidence: its distinctive tokens (sampled); closure_share:
    the share the label's installed closure holds; holders: [(package, share, file bytes, path, root)] of the files
    OUTSIDE the closure holding at least HOLD. Returns (reason, chosen holder or None)."""
    if len(evidence) < MIN_EVIDENCE:
        return "few-evidence", None
    if closure_share >= LACK:
        return "label-holds", None
    if not holders:
        return "nobody-holds", None
    if any(same_family(h[0], label) for h in holders):
        return "label-family-elsewhere", None
    if app_pkg and any(h[0] == app_pkg or same_family(h[0], app_pkg) for h in holders):
        return "app-holds", None
    fams = []
    for h in holders:
        if not any(same_family(h[0], f) for f in fams):
            fams.append(h[0])
    if len(fams) > 1:
        return "ambiguous", None
    best = sorted(holders, key=lambda h: (-h[1], h[2], h[3]))[0]
    return "relabel", best


# ---------------------------------------------------------------- one entry

def source_paths(entry, m, role):
    base = os.path.dirname(entry["files"][role]["origin"])
    root = m.get("sourceRoot") or ""
    return [os.path.normpath(os.path.join(base, root, s)) if isinstance(s, str) else "" for s in m.get("sources", [])]


def label_source(truth, offs, s, e, lab):
    """The source index score.label_units reads a library label's version/file from: the source of that package
    holding most of the unit's bytes (offs: the segments' start offsets)."""
    import bisect
    segs, srcs = truth["segments"], truth["sources"]
    k = max(bisect.bisect_right(offs, s) - 1, 0)
    files = collections.Counter()
    while k < len(segs) and segs[k][0] < e:
        a, b = max(segs[k][0], s), min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
        if b > a:
            files[segs[k][1]] += b - a
        k += 1
    return next((si for si, _ in files.most_common() if srcs[si][0] == lab), None)


def reference_units(entry, labels):
    """{(start, end): unit} over the reference labels' runs, each label labelled with its own numbering by
    score.label_units (scorer 13's truth, this rule off): [(unit, truth row)] once per span."""
    import score
    import truth_audit
    out = {}
    for label in labels:
        p = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
        if not os.path.exists(p):
            continue
        rep = json.load(open(p))["libraryDiscovery"]
        if rep is None:
            continue
        units = score.run_units(rep)
        audit = truth_audit.Audit(entry, CTX["truth"], CTX["cm"])
        rows = score.label_units(CTX["truth"], units, CTX["origins"], audit)
        for u in score.scored_units(units):
            out.setdefault((u["s"], u["e"]), (u, rows[u["i"]]))
    return out


CTX = {}


def entry_layer(entry):
    import families_lib
    import score
    import truth_audit
    truth_audit.RULES14["mapdefect"] = False  # the layer this builds
    fams = families_lib.Families()
    truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), CTX["chm"])
    origins = truth_audit.origin_layer(entry, CTX["om"], CTX["o7m"])
    truth, origins = truth_audit.redecode(entry, truth, origins)
    CTX.update(truth=truth, origins=origins)
    agg = collections.Counter()
    units = reference_units(entry, CTX["labels"])
    cands = []
    for span, (u, row) in sorted(units.items()):
        lab = row["label"]
        agg["units judged"] += 1
        if lab in score.NOT_LIBRARY:
            continue
        agg["library units"] += 1
        agg["library bytes"] += u["b"]
        if "origin" in row or "misattributedFrom" in row:
            agg["skipped: copy origin or scorer 13 relabel (bytes)"] += u["b"]
            continue
        cands.append((span, u, row))
    role = "innerMap" if "innerMap" in entry["files"] else "map"
    smap = json.load(open(lib096.corpus_path(entry, role)))
    paths = source_paths(entry, smap, role)
    # the app's install tree(s): the node_modules the map's sources live in (the first segment of each path)
    tops = sorted({p[:p.index("/node_modules/")] + "/node_modules" for p in paths if "/node_modules/" in p})
    tops = [t for t in tops if os.path.isdir(t)]
    rows, why = [], {}
    if cands and tops:
        stop = os.path.commonpath([os.path.dirname(t) for t in tops])
        ev = node({"mode": "units", "bundle": lib096.corpus_path(entry, "bundle"),
                   "spans": [list(c[0]) for c in cands]})
        ev = [clean(t) for t in ev]
        files = tree_files(tops)
        want = sorted(set().union(*ev)) if ev else []
        res = node({"mode": "files", "files": [f[2] for f in files], "want": want, "cap": FILE_CAP})
        hits = {t: res["hits"][i] for i, t in enumerate(want) if res["hits"][i]}
        many = {want[i] for i in res["many"]}
        df = {t: len({files[fi][0] for fi in fis}) for t, fis in hits.items()}
        texts = {}

        def text(path):
            if path not in texts:
                if len(texts) > 4000:
                    texts.clear()
                texts[path] = open(path, encoding="utf8", errors="replace").read()
            return texts[path]
        closures, closure_text = {}, {}
        app_pkg = truth.get("appPackage")
        index_of = {c[0]: i for i, c in enumerate(cands)}
        offs = [x[0] for x in truth["segments"]]
        for (s, e), u, row in cands:
            lab = row["label"]
            toks = sample(distinctive(ev[index_of[(s, e)]], df, many))
            si = label_source(truth, offs, s, e, lab)
            path = paths[si] if si is not None and si < len(paths) else ""
            marker = "/node_modules/" + lab + "/"
            k = path.rfind(marker)
            root = path[:k + len(marker) - 1] if k >= 0 else None
            if root is None or not os.path.isfile(os.path.join(root, "package.json")):
                why[(s, e)] = "label-not-installed"
                continue
            if len(toks) < MIN_EVIDENCE:
                why[(s, e)] = "few-evidence"
                continue
            if root not in closures:
                closures[root] = closure(root, stop)
            cl = closures[root]
            # token-set prefilter: exact values held by the closure (implies the substring test)
            held_set = sum(1 for t in toks if any(files[fi][1] in cl for fi in hits.get(t, ())))
            if held_set >= LACK * len(toks):
                why[(s, e)] = "label-holds"
                continue
            key = frozenset(cl)
            if key not in closure_text:
                if len(closure_text) > 64:
                    closure_text.clear()
                closure_text[key] = "\n".join(text(f[2]) for f in files if f[1] in cl)
            cshare = share_in(toks, closure_text[key])
            per_file = collections.Counter()
            for t in toks:
                for fi in hits.get(t, ()):
                    if files[fi][1] not in cl:
                        per_file[fi] += 1
            holders = []
            for fi, n in per_file.items():
                if n < CANDIDATE * len(toks):
                    continue
                sh = share_in(toks, text(files[fi][2]))
                if sh >= HOLD:
                    holders.append((files[fi][0], sh, os.path.getsize(files[fi][2]), files[fi][2], files[fi][1]))
            reason, best = decide(lab, toks, cshare, holders, fams.same, app_pkg)
            why[(s, e)] = reason
            if best is None:
                continue
            agg["relabelled units"] += 1
            rows.append([s, e, lab, best[0], version_of(best[4]), os.path.relpath(best[3], best[4]), len(toks),
                         round(1000 * cshare), round(1000 * best[1]), row.get("file")])
    for span, u, row in cands:
        agg[why.setdefault(span, "no-install-tree") + " (bytes)"] += u["b"]
    out = os.path.join(lib096.DATA, "truth", entry["id"] + ".mapdefect14.json.gz")
    with gzip.GzipFile(out, "wb", mtime=0) as fh:
        fh.write(lib096.dump({"schema": SCHEMA, "rows": sorted(rows),
                              "evaluated": sorted([*c[0], why[c[0]]] for c in cands)}).encode())
    return entry["id"], entry["set"], lib096.sha256_file(out), dict(agg)


def init(corpus, labels):
    lib096.use(corpus)
    import truth_audit
    CTX.update(labels=labels, chm=truth_audit.load_chained_manifest(), om=lib096.load_origins_manifest(),
               o7m=truth_audit.load_origins7_manifest(), cm=truth_audit.load_copies_manifest())


def build(labels, jobs):
    corpus = lib096.CORPUS
    entries = [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    entries.sort(key=lambda e: -e["files"]["bundle"].get("bytes", 0))
    with multiprocessing.Pool(jobs, initializer=init, initargs=(corpus, labels)) as pool:
        res = pool.map(entry_layer, entries, chunksize=1)
    agg = collections.defaultdict(collections.Counter)
    for _, s, _, a in res:
        agg[s].update(a)
    man = {"schema": SCHEMA, "corpusSha256": lib096.manifest_sha(), "unitsLabels": labels,
           "entries": {k: sha for k, _, sha, _ in sorted(res)},
           "rules": {"minEvidence": MIN_EVIDENCE, "dfMax": DF_MAX, "lack": LACK, "hold": HOLD,
                     "candidate": CANDIDATE, "sample": SAMPLE, "fileCap": FILE_CAP, "depKeys": list(DEP_KEYS)},
           "bytes": {s: dict(sorted(c.items())) for s, c in sorted(agg.items())},
           "about": "scorer 14 rule `mapdefect` (experiments/137-scorer-mapdefect/scorer14/mapdefect14.py): per "
                    "entry, the sha256 of <DATA>/truth/<id>.mapdefect14.json.gz (rows: unit start, end, old "
                    "package, new package, version, file, evidence tokens, closure share x1000, holder share "
                    "x1000, old file; evaluated: every span the rule read, with its verdict)"}
    open(manifest_path(), "w").write(lib096.dump(man))
    for s, c in sorted(agg.items()):
        for k, v in sorted(c.items()):
            print(f"{s:16s} {v:>14,}  {k}")


if __name__ == "__main__":
    argv = lib096.corpus_flag(sys.argv)
    if argv[1:2] != ["build"] or "--units" not in argv:
        raise SystemExit(__doc__)
    jobs = int(argv[argv.index("--jobs") + 1]) if "--jobs" in argv else 8
    build(argv[argv.index("--units") + 1].split(","), jobs)
