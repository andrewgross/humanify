"""exp112 scorer 7: freeze the origin layer with origins.py's RULES7 (lazybody, chunk) for one corpus.

usage: <venv python> -I origins7.py needs  --corpus C <out.json>        the (package, version) manifests the chunk
                                                                       rule reads (for exp104 fetch_metadata.py)
       <venv python> -I origins7.py owners --corpus C <out.json>       the chunk files' owners (for bundled.py)
       <venv python> -I origins7.py build  --corpus C [--jobs N]       <DATA>/truth/<id>.origins7.json.gz and
                                                                       origins7[-<corpus>].json (their sha256s)
       <venv python> -I origins7.py verify --corpus C [--jobs N]       with every rule OFF, the layer equals the
                                                                       frozen scorer 4-6 origin layer (byte for byte)
       <venv python> -I origins7.py stats  --corpus C                  bytes per origin kind, per set (aggregate)
Reads only the frozen bundle, map and truth, plus the npm registry manifests cached in REGISTRY (filled by
experiments/104-truth-audit/fetch_metadata.py from `needs`) and the license collections cached in BUNDLED
(bundled.py, over the chunk owners). Never the index, never a label."""
import collections
import gzip
import json
import multiprocessing
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv)
import families_lib  # noqa: E402
import origins as O  # noqa: E402

REGISTRY = "/work/exp112-data/registry"
BUNDLED = "/work/exp112-data/bundled"  # bundled.py: a tarball's own third-party license collection
SCHEMA = "exp112-origins7/1"
FAM = families_lib.Families()


def manifest_path(pkg, ver):
    return os.path.join(REGISTRY, pkg.replace("/", "__") + "@" + str(ver or "latest").replace("/", "_") + ".json")


def manifest(pkg, ver):
    for v in ((ver, "latest") if ver else ("latest",)):
        p = manifest_path(pkg, v)
        if os.path.exists(p):
            d = json.load(open(p))
            return None if "__error" in d else d
    return None


def bundled(pkg, ver):
    for v in ((ver, "latest") if ver else ("latest",)):
        p = os.path.join(BUNDLED, pkg.replace("/", "__") + "@" + str(v) + ".json")
        if os.path.exists(p):
            return set(json.load(open(p)).get("bundled") or ())
    return set()


def inputs(pkg, ver):
    """(runtime dependency names, build-input names) of pkg@ver: the npm manifest (version, else latest) and the
    tarball's license collection."""
    m = manifest(pkg, ver) or {}
    runtime = set().union(*[set(m.get(k) or ()) for k in O.RUNTIME_KEYS])
    build = set().union(*[set(m.get(k) or ()) for k in O.BUILD_KEYS]) | bundled(pkg, ver)
    return runtime, build - runtime


def layer7_path(entry):
    return os.path.join(lib096.DATA, "truth", entry["id"] + ".origins7.json.gz")


def manifest7_path():
    return os.path.join(lib096.HERE, "origins7.json" if lib096.CORPUS == "dev" else f"origins7-{lib096.CORPUS}.json")


def load(entry):
    truth = lib096.load_truth(entry)
    with open(lib096.corpus_path(entry, "bundle"), encoding="utf8") as fh:
        text = fh.read()
    smap = json.load(open(lib096.corpus_path(entry, "map")))
    if "innerMap" in entry["files"]:
        srcmap = json.load(open(lib096.corpus_path(entry, "innerMap")))
        segs = O.segments(text, smap, (srcmap, 1))
    else:
        srcmap = smap
        segs = O.segments(text, smap)
    if [[s[0], s[1]] for s in segs] != truth["segments"]:
        raise SystemExit(f"REFUSED: {entry['id']}: re-decoded segments differ from the frozen truth")
    return truth, srcmap, segs


def entries():
    return [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]


def needs_one(entry):
    truth, srcmap, _ = load(entry)
    out = set()
    contents = srcmap.get("sourcesContent") or [None] * len(srcmap["sources"])
    for si, (p, c) in enumerate(zip(srcmap["sources"], contents)):
        pkg, ver = truth["sources"][si][0], truth["sources"][si][1]
        regs = O.marker_regions7(c, True) if c else []
        if any(o and o != pkg for _, o, _, _ in regs):
            out |= {(o, O.path_version(mp, o)) for _, o, mp, _ in regs if o and o != pkg}
        elif "node_modules/" in p and pkg and not pkg.startswith("<"):
            out.add((pkg, ver))
    return sorted(out, key=lambda x: (x[0], x[1] or ""))


HASHED = re.compile(r"^(.+?)-[A-Za-z0-9_]{8}(?:-[A-Za-z0-9_-]{8})?$")
CHUNK_DIR = re.compile(r"(?:^|/)_?chunks/|(?:^|/)libs/")


def owners_one(entry):
    """(owner, version) of every file the chunk rule could name after another package: a rolldown-style hashed
    chunk (`<name>-<8 chars>`) or a file under a chunks/ or libs/ directory, owned by the shipper or by a marker
    region's origin. Only these owners' tarballs are read for their license collection."""
    truth, srcmap, _ = load(entry)
    out = set()
    contents = srcmap.get("sourcesContent") or [None] * len(srcmap["sources"])
    for si, (p, c) in enumerate(zip(srcmap["sources"], contents)):
        pkg, ver = truth["sources"][si][0], truth["sources"][si][1]
        regs = O.marker_regions7(c, True) if c else []
        files = []
        if any(o and o != pkg for _, o, _, _ in regs):
            files = [(o, O.path_version(mp, o), mp.rsplit("node_modules/" + o + "/", 1)[-1])
                     for _, o, mp, _ in regs if o and o != pkg and "node_modules/" + o + "/" in mp]
        elif pkg and "node_modules/" + pkg + "/" in p:
            files = [(pkg, ver, p.rsplit("node_modules/" + pkg + "/", 1)[-1])]
        for o, v, rest in files:
            stem = O.SRC_EXT.sub("", rest.rsplit("/", 1)[-1])
            if HASHED.match(stem) or CHUNK_DIR.search(rest):
                out.add((o, v))
    return out


def build_one(job):
    entry, rules = job
    truth, srcmap, segs = load(entry)
    origins = O.Origins(srcmap["sources"], srcmap.get("sourcesContent"), [s[0] for s in truth["sources"]],
                        rules=rules, versions=[s[1] for s in truth["sources"]], inputs=inputs,
                        same_family=FAM.same)
    names, kinds, runs = O.origin_runs(segs, origins)
    return entry["id"], {"schema": O.SCHEMA, "names": names, "kinds": kinds, "runs": runs, "segments": len(segs)}


def main():
    cmd = ARGV[1]
    jobs = int(ARGV[ARGV.index("--jobs") + 1]) if "--jobs" in ARGV else 8
    es = entries()
    if cmd == "needs":
        with multiprocessing.Pool(jobs) as pool:
            got = set().union(*map(set, pool.map(needs_one, es, chunksize=1)))
        pv = sorted([p, v] for p, v in got if v)
        names = sorted({p for p, v in got})
        json.dump({"names": names, "pv": pv}, open(ARGV[2], "w"))
        print(f"{len(names)} names, {len(pv)} versions -> {ARGV[2]}")
    elif cmd == "owners":
        with multiprocessing.Pool(jobs) as pool:
            got = set().union(*pool.map(owners_one, es, chunksize=1))
        json.dump(sorted(([p, v] for p, v in got), key=lambda x: (x[0], x[1] or "")), open(ARGV[2], "w"))
        print(f"{len(got)} chunk owners -> {ARGV[2]}")
    elif cmd == "verify":
        with multiprocessing.Pool(jobs) as pool:
            res = pool.map(build_one, [(e, None) for e in es], chunksize=1)
        bad = 0
        for eid, layer in res:
            e = next(x for x in es if x["id"] == eid)
            with gzip.open(lib096.origins_path(e), "rt") as fh:
                frozen = json.load(fh)
            if frozen != layer:
                bad += 1
                print("DIFFERS", eid)
        print(f"rules off: {len(res) - bad}/{len(res)} entries equal the frozen origin layer")
        sys.exit(1 if bad else 0)
    elif cmd == "build":
        with multiprocessing.Pool(jobs) as pool:
            res = pool.map(build_one, [(e, dict(O.RULES7)) for e in es], chunksize=1)
        shas = {}
        for eid, layer in res:
            e = next(x for x in es if x["id"] == eid)
            lib096.write_gz_json(layer7_path(e), layer)
            shas[eid] = lib096.sha256_file(layer7_path(e))
        man = {"schema": SCHEMA, "corpusSha256": lib096.manifest_sha(), "rules": dict(O.RULES7),
               "about": "scorer 7 origin layer (experiments/112-test-precision/scorer7/origins7.py): origins.py "
                        "with RULES7; per entry the sha256 of <DATA>/truth/<id>.origins7.json.gz",
               "entries": {k: shas[k] for k in sorted(shas)}}
        open(manifest7_path(), "w").write(lib096.dump(man))
        print(f"froze {len(shas)} entries -> {manifest7_path()}")
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
