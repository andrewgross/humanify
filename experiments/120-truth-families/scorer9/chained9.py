"""exp120 scorer 9, rule `npmchain`: a chained source whose path names a package npm does not have keeps the real
package whose own source map the bundler followed to reach it.

esbuild follows an installed package's `//# sourceMappingURL` map. primevue 4's maps list `../../src/<x>.js`
against `node_modules/@primevue/core/<x>/index.mjs.map`, which resolves to `node_modules/@primevue/src/<x>.js`. That
path is not on disk, and the truth's path rule (truth.package_root) reads it as the package `@primevue/src`. No such
package exists on npm, so no claim can ever match that name, and the label sits in no family. Scorer 5's rule
`chained` covers the same mechanism, but only for a source the truth labelled `<app>`; this one covers a source
whose label is a package name.

Rule (mechanical, no index, no claims): a source the truth labels with a package P is relabelled when
  1. its path, resolved against the bundle's directory, does NOT exist on disk,
  2. P is not a package on npm (the registry answers 404 for `P@latest`; read from the exp104 metadata cache,
     fetched with fetch_metadata.py when missing),
  3. some installed package's own map lists that path (scorer5/chained.py's search: a `*.map` file under the
     app's node_modules, its `sources` resolved against the map's directory and `sourceRoot`), and that package
     H (the first holder by path order) is a package on npm and is not P.
It then takes H, with H's installed version and the path relative to H's node_modules. A source missing a
condition keeps its label. A name that IS on npm (e.g. a chained `node_modules/src/...` path, where `src` happens
to be a real npm package) is never relabelled; those are counted (`realNameNotInstalled`), not changed.

usage: <venv python> -I chained9.py build --corpus dev|apps|bundlers|pop|test|test2
       writes <DATA>/truth/<id>.chained9.json.gz for every sourcemap entry and the manifest chained9[-<corpus>].json
       beside copies.json (the per-entry sha256, plus every npm verdict the rule read). Prints aggregates per set
       only; it never prints app or package names for the private corpora (test, test2)."""
import collections
import gzip
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.normpath(os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, SCORE)
sys.path.insert(0, os.path.normpath(os.path.join(HERE, "../../106-matcher-hygiene/scorer5")))
sys.path.insert(0, os.path.normpath(os.path.join(HERE, "../../104-truth-audit")))
import chained as C  # noqa: E402  (exp106 scorer 5: the package-map search)
import fetch_metadata as FM  # noqa: E402  (exp104: the registry cache and its fetcher)
import lib096  # noqa: E402

REGISTRY = "/work/exp104-data/registry"
SCHEMA = "exp120-chained9/1"
# a name npm can publish: optional @scope/, no leading `.` or `_`, URL-safe characters (legacy names may hold capitals)
VALID = re.compile(r"^(?:@[a-z0-9~-][a-z0-9._~-]*/)?[a-z0-9~-][a-z0-9._~-]*$", re.I)


def manifest_path(corpus=None):
    c = corpus or lib096.CORPUS
    return os.path.join(SCORE, "chained9.json" if c == "dev" else f"chained9-{c}.json")


def layer_path(entry):
    return os.path.join(lib096.DATA, "truth", entry["id"] + ".chained9.json.gz")


class Npm:
    """`name@latest` exists on npm? From the exp104 cache; a name with no cached answer is fetched once."""

    def __init__(self, cache=REGISTRY):
        self.cache = cache
        self.verdicts = {}

    def real(self, name):
        if name not in self.verdicts and not VALID.match(name):
            self.verdicts[name] = False  # npm cannot hold the name at all (validate-npm-package-name)
        if name not in self.verdicts:
            p = FM.path(self.cache, name, "latest")
            if not os.path.exists(p) and FM.get(self.cache, (name, "latest")):
                raise SystemExit(f"REFUSED: the registry gave no answer for {name}@latest")
            self.verdicts[name] = "__error" not in json.load(open(p))
        return self.verdicts[name]


def entry_layer(entry, truth, npm, agg):
    base = os.path.dirname(entry["files"]["bundle"]["origin"])
    smap = json.load(open(lib096.corpus_path(entry, "innerMap" if "innerMap" in entry["files"] else "map")))
    srcs = smap["sources"]
    missing = collections.defaultdict(list)  # resolved path -> source indices
    for i, (s, r) in enumerate(zip(srcs, truth["sources"])):
        pkg = r[0]
        if not isinstance(s, str) or not pkg or pkg.startswith("<"):
            continue
        p = os.path.normpath(os.path.join(base, s))
        if not os.path.exists(p):
            missing[p].append(i)
    if not missing:
        return {}
    agg["package-labelled sources not on disk"] += sum(len(v) for v in missing.values())
    holders = C.map_holders(missing, C.install_dirs(base, srcs))
    out = {}
    for p, idx in sorted(missing.items(), key=lambda kv: kv[1]):
        row = C.holder_row(holders.get(p), p)
        if row is None:
            continue
        for i in idx:
            label = truth["sources"][i][0]
            if row[0] == label:
                continue
            if npm.real(label):
                agg["realNameNotInstalled (kept)"] += 1
                continue
            if not npm.real(row[0]):
                agg["holder not on npm (kept)"] += 1
                continue
            out[str(i)] = [*row, label]
            agg["relabelled to the holder"] += 1
    return out


def build():
    npm = Npm()
    entries = [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    shas = {}
    agg = collections.defaultdict(collections.Counter)
    for e in entries:
        layer = entry_layer(e, lib096.load_truth(e), npm, agg[e["set"]])
        p = layer_path(e)
        with gzip.GzipFile(p, "wb", mtime=0) as fh:
            fh.write(lib096.dump({"schema": SCHEMA, "sources": layer}).encode())
        shas[e["id"]] = lib096.sha256_file(p)
    man = {"schema": SCHEMA, "corpusSha256": lib096.manifest_sha(), "entries": shas,
           "npmVerdicts": dict(sorted(npm.verdicts.items())),
           "about": "scorer 9 rule `npmchain` (experiments/120-truth-families/scorer9/chained9.py): per entry, the "
                    "sha256 of <DATA>/truth/<id>.chained9.json.gz; npmVerdicts = every `name@latest exists on "
                    "npm` answer the rule read"}
    if lib096.CORPUS in lib096.PRIVATE:
        # the verdicts name the private corpus's packages: keep only their count in the committed manifest
        man["npmVerdicts"] = {"count": len(npm.verdicts), "sha256": lib096.sha256_bytes(
            lib096.dump(dict(sorted(npm.verdicts.items()))).encode())}
    open(manifest_path(), "w").write(lib096.dump(man))
    for s, c in sorted(agg.items()):
        for k, v in sorted(c.items()):
            print(f"{s:16s} {v:>7}  {k}")


if __name__ == "__main__":
    argv = lib096.corpus_flag(sys.argv)
    if argv[1:2] != ["build"]:
        raise SystemExit(__doc__)
    build()
