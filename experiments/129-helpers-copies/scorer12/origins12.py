"""exp129 scorer 12: freeze the origin layer with origins.py's RULES12 (scorer 10's lazybody + chunk + via, plus
`patched`: a marker path may hold `=`, a pnpm patched dependency) for one corpus. exp125's origins10.py builder with
RULES12; reads nothing new.

usage: <venv python> -I origins12.py build  --corpus C [--jobs N]   <DATA>/truth/<id>.origins12.json.gz and
                                                                   origins12[-<corpus>].json (their sha256s)
       <venv python> -I origins12.py verify --corpus C [--jobs N]   with `patched` off the builder reproduces the
                                                                   frozen scorer 10 layer byte for byte
Never the index, never a label."""
import gzip
import json
import multiprocessing
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../125-nested-chunk-publisher/scorer10"))
sys.path.insert(0, os.path.join(HERE, "../../112-test-precision/scorer7"))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import origins7 as S7  # noqa: E402  (applies --corpus via lib096.corpus_flag)
import origins10 as S10  # noqa: E402
import lib096  # noqa: E402
import origins as O  # noqa: E402

SCHEMA = "exp129-origins12/1"
RULES = dict(O.RULES12)


def layer12_path(entry):
    return os.path.join(lib096.DATA, "truth", entry["id"] + ".origins12.json.gz")


def manifest12_path():
    return os.path.join(lib096.HERE, "origins12.json" if lib096.CORPUS == "dev" else f"origins12-{lib096.CORPUS}.json")


def build_one(entry):
    truth, srcmap, segs = S7.load(entry)
    origins = O.Origins(srcmap["sources"], srcmap.get("sourcesContent"), [s[0] for s in truth["sources"]],
                        rules=dict(RULES), versions=[s[1] for s in truth["sources"]], inputs=S7.inputs,
                        same_family=S7.FAM.same)
    names, kinds, runs = O.origin_runs(segs, origins)
    return entry["id"], {"schema": O.SCHEMA, "names": names, "kinds": kinds, "runs": runs, "segments": len(segs)}


def main():
    cmd = S7.ARGV[1]
    jobs = int(S7.ARGV[S7.ARGV.index("--jobs") + 1]) if "--jobs" in S7.ARGV else 4
    es = S7.entries()
    if cmd == "verify":
        RULES["patched"] = False
    with multiprocessing.Pool(jobs) as pool:
        res = dict(pool.map(build_one, es, chunksize=1))
    if cmd == "build":
        shas = {}
        for e in es:
            lib096.write_gz_json(layer12_path(e), res[e["id"]])
            shas[e["id"]] = lib096.sha256_file(layer12_path(e))
        man = {"schema": SCHEMA, "corpusSha256": lib096.manifest_sha(), "rules": dict(RULES),
               "about": "scorer 12 origin layer (experiments/129-helpers-copies/scorer12/origins12.py): origins.py "
                        "with RULES12; per entry the sha256 of <DATA>/truth/<id>.origins12.json.gz",
               "entries": {k: shas[k] for k in sorted(shas)}}
        open(manifest12_path(), "w").write(lib096.dump(man))
        moved = 0
        for e in es:
            with gzip.open(S10.layer10_path(e), "rt") as fh:
                moved += json.load(fh) != res[e["id"]]
        print(f"froze {len(shas)} entries -> {manifest12_path()}; {moved} differ from scorer 10's layer")
    elif cmd == "verify":
        bad = 0
        for e in es:
            with gzip.open(S10.layer10_path(e), "rt") as fh:
                if json.load(fh) != res[e["id"]]:
                    bad += 1
                    print("DIFFERS", e["id"])
        print(f"patched off: {len(es) - bad}/{len(es)} entries equal the frozen scorer 10 layer")
        sys.exit(1 if bad else 0)
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
