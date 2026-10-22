"""exp136: simulate a builder EVIDENCE CHECK on every unit row (drop a row neither its file nor its package holds the
unit's name evidence of) on the claims of a label, without rebuilding the index. For every index SEED claim (judge13 rows, slot c0, any verdict), read the unit's name evidence from the bundle and ask
whether the claimer's index rows of the print the lookup read (the whole print; else the unit's largest function
print the claimer holds) sit in files that hold it, and whether the claimer's installed package (the union of its files) does
(seedcheck.row_state / whohas). A seed whose claimer rows ALL lack the evidence and whose package lacks it is the
seed the check would remove (its rows dropped): bytes per verdict (right = the scorer's right family, wrong =
wrongFamily). Propagation off a removed seed is not followed (a lower bound on both sides).

usage: python3 -I evsim.py <corpus> <label> <judge.jsonl> [--json out]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import dcheck  # noqa: E402
import lib096  # noqa: E402
import seedcheck  # noqa: E402
import whohas  # noqa: E402


def main():
    corpus, label, path = sys.argv[1:4]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    import score
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs, units, slices = {}, {}, {}
    agg = collections.Counter()
    pkg_cache = {}
    dump = open(sys.argv[sys.argv.index("--dump") + 1], "w") if "--dump" in sys.argv else None  # scratch: names
    seeds = collections.defaultdict(list)
    for line in open(path):
        r = json.loads(line)
        c = r.get("c0")
        if c and c["rule"] == "seed" and c["evidence"] in ("index", "queueIndex"):
            seeds[r["entry"]].append(r)
    toks_of = {}
    for eid, rs in seeds.items():
        for r, t in zip(rs, dcheck.acorn_evidence(dcheck.bundle_path(eid), [[r["start"], r["end"]] for r in rs])):
            toks_of[(eid, r["start"], r["end"])] = t
    for r in (r for rs in seeds.values() for r in rs):
        c = r["c0"]
        verdict = ("wrong" if c["verdict"] == "wrongFamily" else "right" if c["verdict"] in ("right", "family", "origin")
                   else "other") + ("" if c["verdict"] in ("right", "wrongFamily") else "/" + c["verdict"])
        eid = r["entry"]
        if eid not in units:
            units.clear()
            doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", eid + ".json")))
            slices[eid] = doc["toolchain"]["slice"]
            units[eid] = {(u["start"], u["end"]): u for u in doc["libraryDiscovery"].get("unitRows", ())}
        u = units[eid].get((r["start"], r["end"]))
        sl = slices[eid]
        if u is None or "whole" not in u:
            agg[(r["set"], verdict, "no-prints")] += r["bytes"]
            continue
        if sl not in ixs:
            ixs[sl] = score.load_index(pipe["indexDir"], sl, pipe["indexId"])
        ix = ixs[sl]
        toks = toks_of[(eid, r["start"], r["end"])]
        if len(toks) < 3:
            agg[(r["set"], verdict, "kept: no evidence")] += r["bytes"]
            continue
        W = c["package"]
        h = u["whole"]["hash"]
        if not any(row[0] == W for row in ix.units.get(h, ())):
            fns = sorted((f for f in u.get("functions", ()) if f["nodes"] >= 30
                          and any(row[0] == W for row in ix.units.get(f["hash"], ()))), key=lambda f: -f["nodes"])
            h = fns[0]["hash"] if fns else None
        rs = seedcheck.row_state(ix, h, W, toks) if h else "no-row"
        if rs != "lacks":
            agg[(r["set"], verdict, f"kept: row {rs}")] += r["bytes"]
            continue
        key = (W, tuple(sorted(toks)))
        if key not in pkg_cache:
            pkg_cache[key] = any(  # the UNION of the package's files (a unit can span several of them)
                sum(any(t in text for p, _, text in whohas.files_of(wd) if p == W) for t in toks) >= 0.5 * len(toks)
                for wd in whohas.claimer_dirs(W))
        agg[(r["set"], verdict, "kept: package holds" if pkg_cache[key] else "DROPPED")] += r["bytes"]
        if dump and not pkg_cache[key]:
            dump.write(json.dumps({"entry": eid, "set": r["set"], "verdict": verdict, "claimer": W, "bytes": r["bytes"],
                                   "start": r["start"], "end": r["end"], "hash": h, "toks": sorted(toks)}) + "\n")
    out = [{"set": k[0], "verdict": k[1], "fate": k[2], "kb": round(v / 1e3, 1)} for k, v in sorted(agg.items())]
    for o in out:
        print(f"{o['set']:14} {o['verdict']:10} {o['fate']:26} {o['kb']:10.1f} KB")
    if "--json" in sys.argv:
        json.dump(out, open(sys.argv[sys.argv.index("--json") + 1], "w"), indent=1)


if __name__ == "__main__":
    main()
