"""exp125 scorer 10: freeze the origin layer with origins.py's RULES10 (scorer 7's lazybody + chunk, plus `via`) for
one corpus. Reuses exp112's origins7.py readers (the frozen bundle, map and truth; the cached registry manifests and
license collections): `via` reads nothing new.

usage: <venv python> -I origins10.py build  --corpus C [--jobs N]   <DATA>/truth/<id>.origins10.json.gz and
                                                                   origins10[-<corpus>].json (their sha256s)
       <venv python> -I origins10.py verify --corpus C [--jobs N]   per segment, the scorer 10 layer with each
                                                                   `chunk/via:<R>` kind read as `chunk` equals the
                                                                   frozen scorer 7 layer (origin and kind)
Never the index, never a label."""
import gzip
import json
import multiprocessing
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../112-test-precision/scorer7"))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import origins7 as S7  # noqa: E402  (applies --corpus via lib096.corpus_flag)
import lib096  # noqa: E402
import origins as O  # noqa: E402

SCHEMA = "exp125-origins10/1"


def layer10_path(entry):
    return os.path.join(lib096.DATA, "truth", entry["id"] + ".origins10.json.gz")


def manifest10_path():
    return os.path.join(lib096.HERE, "origins10.json" if lib096.CORPUS == "dev" else f"origins10-{lib096.CORPUS}.json")


def build_one(entry):
    truth, srcmap, segs = S7.load(entry)
    origins = O.Origins(srcmap["sources"], srcmap.get("sourcesContent"), [s[0] for s in truth["sources"]],
                        rules=dict(O.RULES10), versions=[s[1] for s in truth["sources"]], inputs=S7.inputs,
                        same_family=S7.FAM.same)
    names, kinds, runs = O.origin_runs(segs, origins)
    return entry["id"], {"schema": O.SCHEMA, "names": names, "kinds": kinds, "runs": runs, "segments": len(segs)}


def per_segment(layer, collapse):
    out, runs = [], layer["runs"]
    for j, (k, ni, ki) in enumerate(runs):
        end = runs[j + 1][0] if j + 1 < len(runs) else layer["segments"]
        kind = layer["kinds"][ki] if ki >= 0 else None
        if collapse and kind and kind.startswith(O.VIA):
            kind = "chunk"
        out.extend([(layer["names"][ni] if ni >= 0 else None, kind)] * (end - k))
    return out


def main():
    cmd = S7.ARGV[1]
    jobs = int(S7.ARGV[S7.ARGV.index("--jobs") + 1]) if "--jobs" in S7.ARGV else 4
    es = S7.entries()
    if cmd == "build":
        with multiprocessing.Pool(jobs) as pool:
            res = pool.map(build_one, es, chunksize=1)
        shas = {}
        for eid, layer in res:
            e = next(x for x in es if x["id"] == eid)
            lib096.write_gz_json(layer10_path(e), layer)
            shas[eid] = lib096.sha256_file(layer10_path(e))
        man = {"schema": SCHEMA, "corpusSha256": lib096.manifest_sha(), "rules": dict(O.RULES10),
               "about": "scorer 10 origin layer (experiments/125-nested-chunk-publisher/scorer10/origins10.py): "
                        "origins.py with RULES10; per entry the sha256 of <DATA>/truth/<id>.origins10.json.gz",
               "entries": {k: shas[k] for k in sorted(shas)}}
        open(manifest10_path(), "w").write(lib096.dump(man))
        print(f"froze {len(shas)} entries -> {manifest10_path()}")
    elif cmd == "verify":
        bad = gained = 0
        for e in es:
            with gzip.open(layer10_path(e), "rt") as fh:
                l10 = json.load(fh)
            with gzip.open(S7.layer7_path(e), "rt") as fh:
                l7 = json.load(fh)
            if per_segment(l10, True) != per_segment(l7, False):
                bad += 1
                print("DIFFERS", e["id"])
            gained += sum(1 for _, k in per_segment(l10, False) if k and k.startswith(O.VIA))
        print(f"via read as chunk: {len(es) - bad}/{len(es)} entries equal the frozen scorer 7 layer; "
              f"{gained} segments carry a via")
        sys.exit(1 if bad else 0)
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
