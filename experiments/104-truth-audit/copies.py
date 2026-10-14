"""exp104: the COPIES layer of the exp096 truth — vendored / pre-bundled copies that leave no marker and no vendored
directory (so origins.py cannot see them), found by a dependency-gated code match.

usage: <venv python> -I copies.py <reference label> <registry cache> <families-derived.json> [--jobs N]
                                  [--corpus dev|test]
writes <DATA>/truth/<id>.copies.json.gz per source-mapped entry and experiments/096-library-matching-score/
       copies.json (copies-test.json for the test corpus: their sha256s, the reference label, index and the rule's
       constants). The rule was designed on dev only and is applied to test unchanged.

A statement unit whose truth label is the SHIPPER S (version v from the frozen truth) is a copy of ORIGIN O when
all of these hold:
  code       the reference index's prints of O's family cover >= COVER of the unit's nodes: its whole print is
             held by an O-family row, else O-family function prints (largest non-overlapping first, each
             >= MIN_NODES nodes) cover it. A print held by more than MAX_FAMILIES families is shared helper code
             and is not evidence. O's family is not S's.
  dependency S@v's own npm manifest declares O or a package linked to it (dependencies, devDependencies, peer,
             optional or bundled: a bundled copy is usually a devDependency inlined at build time). This is what
             separates an ORIGIN from a CONSUMER: d3-delaunay's dist bundles delaunator, so delaunator's code is
             in d3-delaunay's index rows, but delaunator does not depend on d3-delaunay. The match alone is the
             matcher's own evidence (origins.py, NOT evidence); the dependency is not.
  named      the shipped file the unit maps into (its sourcesContent, S's published bytes) names O or the declaring
             dependency as a word: a license banner, a comment, a module path a bundler kept (openpgp's dist
             carries tweetnacl's and fflate's names; cytoscape's carries lodash's). Without it a shared helper that
             happens to match a dependency's print would pass (openai's internal tslib helper vs zod).
  unique     exactly one family passes all of these (the largest cover; a tie is no copy).
  mapped     the unit has a source-map segment of its own (a unit carried by an earlier segment's attribution is
             the `carried` rule's question, not this one's).
Families are the derived ones (families-derived.json + families.json). The units and prints are the REFERENCE
label's (one fixed run, one fixed index); the layer is frozen as byte ranges, and score.py gives a unit of any
label the origin when most of its bytes lie in one origin's ranges under the same shipper.
"""
import bisect
import collections
import json
import multiprocessing
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.join(HERE, "../096-library-matching-score")
sys.path.insert(0, SCORE)
import lib096  # noqa: E402
import mmlib  # noqa: E402
import score  # noqa: E402
from families_lib import base_family  # noqa: E402
from lib096 import dump, sha256_file, write_gz_json  # noqa: E402

SCHEMA = "exp104-copies/1"
COVER = 0.5
MIN_NODES = 30  # the structural rule's print floor (mmlib.RULES["min_nodes"])
MAX_FAMILIES = 3  # a print more families hold is shared helper code, not evidence (the lookup's max_packages)
DEP_KEYS = ("dependencies", "devDependencies", "peerDependencies", "optionalDependencies", "bundleDependencies",
            "bundledDependencies")
CTX = {}


def manifest_deps(cache, pkg, ver):
    p = os.path.join(cache, pkg.replace("/", "__") + "@" + str(ver) + ".json")
    if not os.path.exists(p):
        return None
    d = json.load(open(p))
    if "__error" in d:
        return None
    out = set()
    for k in DEP_KEYS:
        v = d.get(k) or {}
        out.update(v if isinstance(v, list) else v.keys())
    return out


def named(text, name):
    return re.search(r"(?<![\w-])" + re.escape(name) + r"(?![\w-])", text) is not None


def declared(dep, origin):
    """The dependency names the origin: the same package, a families.json LISTED family or fork (not merely the
    same npm scope: @typescript-eslint/eslint-plugin does not declare @typescript-eslint/typescript-estree), or a
    derived link (fork / rename / one repository)."""
    if dep == origin or CTX["fams"].linked(dep, origin):
        return True
    a, b = base_family(dep), base_family(origin)
    return a == b and not a.startswith("scope:")


def owner_cover(ix, u, same):
    """{owner package: nodes its family's index rows cover}: the whole print if an owner of its family holds it,
    else the largest non-overlapping function prints (>= MIN_NODES) one of its family holds."""
    whole, fns = mmlib.prints(u)

    def owners(h):
        o = {r[0] for r in ix.units.get(h, ())}
        return o if len({base_family(x) for x in o}) <= MAX_FAMILIES else set()
    whole_owners = owners(whole[2]) if whole[3] >= MIN_NODES else set()
    fn_owners = [(f, owners(f[2])) for f in fns if f[3] >= MIN_NODES]
    cands = whole_owners.union(*(o for _, o in fn_owners))
    cov = {}
    for o in cands:
        if any(same(o, w) for w in whole_owners):
            cov[o] = whole[3]
        else:
            cov[o] = mmlib.greedy_cover([f for f, os_ in fn_owners if any(same(o, x) for x in os_)])
    return cov


def entry_copies(entry):
    label, cache = CTX["label"], CTX["cache"]
    same = CTX["same"]
    doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")))
    rep = doc["libraryDiscovery"]
    rows, stats = [], collections.Counter()
    if rep is not None:
        pipeline = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
        ix = score.load_index(pipeline["indexDir"], doc["toolchain"]["slice"], pipeline["indexId"])
        truth = lib096.load_truth(entry)
        segs = truth["segments"]
        offs = [s[0] for s in segs]
        mp = lib096.corpus_path(entry, "innerMap" if "innerMap" in entry["files"] else "map")
        contents = json.load(open(mp)).get("sourcesContent") or []
        units = score.run_units(rep)
        labels = score.label_units(truth, units, lib096.load_origins(entry, CTX["om"]))
        for u in score.statement_units(units):
            lab = labels[u["i"]]
            if lab["label"] in score.NOT_LIBRARY or "origin" in lab or "w" not in u:
                continue
            if bisect.bisect_left(offs, u["s"]) == bisect.bisect_left(offs, u["e"]):
                continue  # carried: not this rule's question
            cov = {o: n for o, n in owner_cover(ix, u, same).items()
                   if not same(o, lab["label"]) and n >= COVER * max(u["w"][1], 1)}
            if not cov:
                continue
            stats["codeMatch"] += u["b"]
            deps = manifest_deps(cache, lab["pkg"], lab.get("version"))
            if deps is None:
                stats["noManifest"] += u["b"]
                continue
            ok = {o: (n, sorted(d for d in deps if declared(d, o))) for o, n in cov.items()}
            ok = {o: v for o, v in ok.items() if v[1]}
            if not ok:
                stats["notADependency"] += u["b"]
                continue
            si = segs[bisect.bisect_right(offs, u["s"]) - 1][1]
            text = contents[si] if si < len(contents) and contents[si] else ""
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
    p = os.path.join(lib096.DATA, "truth", entry["id"] + ".copies.json.gz")
    write_gz_json(p, {"schema": SCHEMA, "reference": label, "ranges": rows, "stats": dict(stats)})
    return entry["id"], sha256_file(p), dict(stats)


def init(label, cache, fam_path, corpus):
    lib096.use(corpus)
    import families_lib
    fams = families_lib.Families(fam_path)
    CTX.update(label=label, cache=cache, same=fams.same, fams=fams,
               om=lib096.load_origins_manifest())


def main():
    import families_lib
    argv = lib096.corpus_flag(sys.argv)
    label, cache, fam_path = argv[1:4]
    if os.path.abspath(fam_path) != os.path.abspath(families_lib.DERIVED):
        raise SystemExit("REFUSED: the copies layer is built with the scorer's own families-derived.json")
    jobs = int(argv[argv.index("--jobs") + 1]) if "--jobs" in argv else 4
    todo = [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    with multiprocessing.Pool(jobs, initializer=init, initargs=(label, cache, fam_path, lib096.CORPUS)) as pool:
        res = pool.map(entry_copies, todo, chunksize=1)
    pipeline = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    tot = collections.Counter()
    for _, _, s in res:
        tot.update(s)
    open(lib096.CORPORA[lib096.CORPUS]["copies"], "w").write(dump({
        "schema": SCHEMA, "corpusSha256": lib096.manifest_sha(), "reference": label,
        "indexId": pipeline["indexId"], "cover": COVER, "minNodes": MIN_NODES, "maxFamilies": MAX_FAMILIES,
        "familiesSha256": families_lib.derived_sha(), "bytes": dict(sorted(tot.items())),
        "entries": {k: sha for k, sha, _ in sorted(res)}}))
    print(dict(tot))


if __name__ == "__main__":
    main()
