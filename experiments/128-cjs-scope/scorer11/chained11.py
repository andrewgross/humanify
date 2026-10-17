"""exp128 scorer 11, rule `notinstalled`: a chained source labelled with a package that is NOT INSTALLED in the app
keeps the real package whose own source map the bundler followed to reach it — scorer 9's `npmchain` without its
"P is not on npm" condition.

Scorer 9 left a chained `node_modules/src/...` or `node_modules/packages/...` path alone because `src` and `packages`
happen to be real npm names (chained9.py's `realNameNotInstalled`). Judging bundled CommonJS modules (scorer 10's
factory scope) put them in front of the scorer: async-validator's map lists `../src/<x>.ts`, Angular 5's
`packages/<x>/esm5/...`, typeorm's and rxjs's `../src/...`, all resolved under the app's node_modules to a package
dir that does not exist. A claim naming async-validator there was judged WRONG against the label `src`.

Rule (mechanical, no index, no claims): a source the truth labels with a package P (after scorer 9's layer) is
relabelled when
  1. its path, resolved against the bundle's directory, does NOT exist on disk,
  2. P is not installed: no `<dir>/P/package.json` under any of the app's node_modules dirs (chained.install_dirs),
  3. some installed package's own map lists that path (scorer 5's search, plus esbuild's reading of `sourceRoot`
     against the map file's path: map_holders), and that package H (the first holder by
     path order) is a package on npm and is not P.
It then takes H, with H's installed version and the path relative to H's node_modules. Rows carry the old label and
apply only to a source still carrying it.

Second case, same rule shape: a source written as a URL (`raw-loader:/home/runner/work/<pkg>/...css?raw`) that the
truth gave `<app>` and an installed package's own map lists VERBATIM takes that package (custom-electron-titlebar's
inlined theme CSS; scorer 5's `chained` resolves sources as paths, so a URL never matched).

usage: <venv python> -I chained11.py build --corpus dev|apps|bundlers|pop|remin|test|test2
       writes <DATA>/truth/<id>.chained11.json.gz per sourcemap entry and chained11[-<corpus>].json beside
       copies.json. Prints aggregates per set only (no names)."""
import collections
import gzip
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.normpath(os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, SCORE)
sys.path.insert(0, os.path.normpath(os.path.join(HERE, "../../120-truth-families/scorer9")))
import chained9 as C9  # noqa: E402  (exp120: Npm verdicts; it imports scorer 5's map search as C)
import lib096  # noqa: E402
import truth_audit  # noqa: E402

C = C9.C
SCHEMA = "exp128-chained11/2"
# a source written as a URL (`raw-loader:/home/runner/...`, `webpack://...`): a map lists it verbatim, so it is
# matched verbatim, never resolved as a path
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


def manifest_path(corpus=None):
    c = corpus or lib096.CORPUS
    return os.path.join(SCORE, "chained11.json" if c == "dev" else f"chained11-{c}.json")


def map_holders(missing, nm):
    """scorer 5's search (chained.map_holders), resolving each map's `sources` BOTH as the spec writes it (against
    the map's directory and `sourceRoot`) and as esbuild 0.28 resolved typeorm's (`sourceRoot` "../.." against the
    map FILE's path: one level higher). Holders in path order, as scorer 5's."""
    holders = collections.defaultdict(list)
    stems = {os.path.basename(p).encode() for p in missing}
    for mp in C.package_maps(nm):
        try:
            if os.path.getsize(mp) > 64 << 20:
                continue
            raw = open(mp, "rb").read()
            if not any(st in raw for st in stems):
                continue
            m = json.loads(raw)
        except (OSError, ValueError, UnicodeDecodeError):
            continue
        if not isinstance(m, dict) or not isinstance(m.get("sources"), list):
            continue
        root = m.get("sourceRoot") or ""
        bases = [os.path.join(os.path.dirname(mp), root)] + ([os.path.join(mp, root)] if root else [])
        for s in m["sources"]:
            if not isinstance(s, str):
                continue
            if SCHEME.match(s):
                if s in missing and mp not in holders[s]:
                    holders[s].append(mp)
                continue
            for b in bases:
                p = os.path.normpath(os.path.join(b, s))
                if p in missing and mp not in holders[p]:
                    holders[p].append(mp)
    return holders


def installed(pkg, dirs):
    return any(os.path.exists(os.path.join(d, pkg, "package.json")) for d in dirs)


def entry_layer(entry, truth, npm, agg):
    base = os.path.dirname(entry["files"]["bundle"]["origin"])
    srcs = json.load(open(lib096.corpus_path(entry, "innerMap" if "innerMap" in entry["files"] else "map")))["sources"]
    dirs = C.install_dirs(base, srcs)
    missing = collections.defaultdict(list)
    for i, (s, r) in enumerate(zip(srcs, truth["sources"])):
        pkg = r[0]
        if not isinstance(s, str) or not pkg:
            continue
        if pkg == "<app>" and SCHEME.match(s):
            missing[s].append(i)  # a URL source the truth gave the app: whose map wrote it? (verbatim)
            continue
        if pkg.startswith("<"):
            continue
        p = os.path.normpath(os.path.join(base, s))
        if not os.path.exists(p) and not installed(pkg, dirs):
            missing[p].append(i)
    if not missing:
        return {}
    agg["candidate sources (not installed / app URL)"] += sum(len(v) for v in missing.values())
    holders = map_holders(missing, dirs)
    out = {}
    for p, idx in sorted(missing.items(), key=lambda kv: kv[1]):
        row = C.holder_row(holders.get(p), p)
        if row is not None and SCHEME.match(p):
            row[2] = p
        if row is None:
            agg["no holder map (kept)"] += len(idx)
            continue
        for i in idx:
            label = truth["sources"][i][0]
            if row[0] == label:
                continue
            if not npm.real(row[0]):
                agg["holder not on npm (kept)"] += 1
                continue
            out[str(i)] = [*row, label]
            agg["relabelled to the holder"] += 1
    return out


def build():
    npm = C9.Npm()
    chm = truth_audit.load_chained_manifest()
    entries = [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    shas = {}
    agg = collections.defaultdict(collections.Counter)
    for e in entries:
        # scorer 9's corrected source table (scorer 5 + npmchain), WITHOUT this layer
        rules = dict(truth_audit.RULES11)
        truth_audit.RULES11["notinstalled"] = False
        truth = truth_audit.patch_sources(e, lib096.load_truth(e), chm)
        truth_audit.RULES11.update(rules)
        layer = entry_layer(e, truth, npm, agg[e["set"]])
        p = os.path.join(lib096.DATA, "truth", e["id"] + ".chained11.json.gz")
        with gzip.GzipFile(p, "wb", mtime=0) as fh:
            fh.write(lib096.dump({"schema": SCHEMA, "sources": layer}).encode())
        shas[e["id"]] = lib096.sha256_file(p)
    man = {"schema": SCHEMA, "corpusSha256": lib096.manifest_sha(), "entries": shas,
           "about": "scorer 11 rule `notinstalled` (experiments/128-cjs-scope/scorer11/chained11.py): per entry, "
                    "the sha256 of <DATA>/truth/<id>.chained11.json.gz"}
    open(manifest_path(), "w").write(lib096.dump(man))
    for s, c in sorted(agg.items()):
        for k, v in sorted(c.items()):
            print(f"{s:16s} {v:>7}  {k}")


if __name__ == "__main__":
    argv = lib096.corpus_flag(sys.argv)
    if argv[1:2] != ["build"]:
        raise SystemExit(__doc__)
    build()
