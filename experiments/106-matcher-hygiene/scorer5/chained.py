"""exp106 scorer 5, rule `chained`: freeze, per corpus entry, the truth sources a bundler reached through a
package's OWN source map (esbuild chains a node_modules file's `//# sourceMappingURL` map: Angular's
fesm2022/*.mjs.map lists ../../../../packages/compiler/src/*.ts, which resolves OUT of node_modules into a path the
app's tree does not have). Such a source is labelled `<app>` by the truth (corpus.sources_table: no node_modules/
in its path) though no app author wrote it.

Rule (mechanical, no index, no claims): a source the truth labels `<app>` whose path, resolved against the bundle's
directory, does NOT exist on disk, and which some installed package's own map lists (a `*.map` file under the
app's node_modules, its `sources` resolved against the map's directory and `sourceRoot`), takes THAT package (the
package holding the map; version from its package.json). Several packages listing it: the first by path order
first by path order. A missing `<app>` source no package map lists stays `<app>`: a missing file alone is no
evidence (wrangler's own monorepo sources are not on disk either). Rollup's `?commonjs-*` virtual ids are rule
`virtual`'s (score time), not this one's.

usage: <venv python> -I chained.py build --corpus dev|test   writes <DATA>/truth/<id>.chained.json.gz for every
       sourcemap entry and the sha manifest (chained.json / chained-test.json beside copies.json). Prints aggregates
       only (counts per set): never per-app names for the test corpus."""
import collections
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.normpath(os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, SCORE)
import lib096  # noqa: E402
import truth as T  # noqa: E402  (exp095 misses: package_root)



def package_maps(nm_dirs):
    for d in nm_dirs:
        if not os.path.isdir(d):
            continue
        for root, dirs, files in os.walk(d):
            dirs.sort()
            for f in sorted(files):
                if f.endswith(".map"):
                    yield os.path.join(root, f)


def entry_layer(entry, truth):
    base = os.path.dirname(entry["files"]["bundle"]["origin"])
    smap = json.load(open(lib096.corpus_path(entry, "innerMap" if "innerMap" in entry["files"] else "map")))
    srcs = smap["sources"]
    missing = collections.defaultdict(list)  # resolved path -> source indices (a map may list one path twice)
    for i, (s, r) in enumerate(zip(srcs, truth["sources"])):
        if r[0] == "<app>" and "?commonjs-" not in s and not os.path.exists(os.path.normpath(os.path.join(base, s))):
            missing[os.path.normpath(os.path.join(base, s))].append(i)
    if not missing:
        return {}, 0
    # where the app's packages live: the node_modules prefixes of the sources that DO resolve into a package
    nm = sorted({os.path.normpath(os.path.join(base, s)).rsplit("/node_modules/", 1)[0] + "/node_modules"
                 for s in srcs if "node_modules/" in s})
    holders = collections.defaultdict(list)
    stems = {os.path.basename(p).encode() for p in missing}
    for mp in package_maps(nm):
        try:
            if os.path.getsize(mp) > 64 << 20:
                continue
            raw = open(mp, "rb").read()
            if not any(st in raw for st in stems):
                continue
            m = json.loads(raw)
        except (OSError, ValueError, UnicodeDecodeError):
            continue
        d = os.path.join(os.path.dirname(mp), m.get("sourceRoot") or "") if isinstance(m, dict) else None
        if d is None or not isinstance(m.get("sources"), list):
            continue
        for s in m["sources"]:
            if isinstance(s, str):
                p = os.path.normpath(os.path.join(d, s))
                if p in missing:
                    holders[p].append(mp)
    out = {}
    for p, idx in sorted(missing.items(), key=lambda kv: kv[1]):
        maps = holders.get(p)
        pr = T.package_root(maps[0]) if maps else None
        if pr is None:
            continue
        pkg, root, _ = pr
        try:
            ver = json.load(open(os.path.join(root, "package.json"))).get("version")
        except (OSError, ValueError):
            ver = None
        rel = os.path.relpath(p, os.path.dirname(root))
        for i in idx:
            out[str(i)] = [pkg, ver, rel]
    return out, sum(len(v) for v in missing.values())


def build():
    entries = [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    shas = {}
    agg = collections.Counter()
    for e in entries:
        layer, n_missing = entry_layer(e, lib096.load_truth(e))
        agg[(e["set"], "missing <app> sources")] += n_missing
        p = os.path.join(lib096.DATA, "truth", e["id"] + ".chained.json.gz")
        with gzip.GzipFile(p, "wb", mtime=0) as fh:
            fh.write(lib096.dump({"schema": "exp106-chained/1", "sources": layer}).encode())
        shas[e["id"]] = lib096.sha256_file(p)
        for v in layer.values():
            agg[(e["set"], "relabelled to a package")] += 1

    man = {"schema": "exp106-chained/1", "corpusSha256": lib096.manifest_sha(), "entries": shas,
           "about": "scorer 5 rule `chained` (experiments/106-matcher-hygiene/scorer5/chained.py): per entry, "
                    "the sha256 of <DATA>/truth/<id>.chained.json.gz"}
    name = "chained.json" if lib096.CORPUS == "dev" else "chained-test.json"
    open(os.path.join(SCORE, name), "w").write(lib096.dump(man))
    for k, v in sorted(agg.items()):
        print(f"{v:>6}  {k}")


if __name__ == "__main__":
    argv = lib096.corpus_flag(sys.argv)
    if argv[1:2] != ["build"]:
        raise SystemExit(__doc__)
    build()
