"""exp142: every library unit scorer 14 counts as NOT named (recall's complement: unclaimed, or claimed as the wrong
family / app), with the truth row it judges it against: shipper package T, version V, file F, origin O. Exactly
score.py's score_map unit set and verdicts (both scopes), so per-set sums reproduce the cards' libraryBytes -
namedBytes. Output stays OUTSIDE the repo (it names packages). Refuses the frozen judges (test, test2, remin2).

usage: <venv python> -I misses.py <corpus> <label> <out.jsonl>
Each row: entry, set, bytes, scope, kind, pkg, version, file, origin, claimed (package or null), verdict, the
index slice the bundle reads, and for a missed unit with prints its whole print [hash, nodes] and function prints."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402


def main():
    corpus, label, out = sys.argv[1:4]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never diagnose a frozen judge")
    lib096.use(corpus)
    import score
    import truth_audit
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    run_dir = os.path.join(lib096.DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    fh = open(out, "w")
    for entry in lib096.load_manifest()["entries"]:
        if entry["truth"]["method"] != "sourcemap":
            continue
        if pipeline.get("sets") and entry["set"] not in pipeline["sets"]:
            continue
        doc = json.load(open(os.path.join(run_dir, "reports", entry["id"] + ".json")))
        rep, slice_ = doc["libraryDiscovery"], doc["toolchain"]["slice"]
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        if rep is None:
            continue  # floored bundle: scored by score_floor, no units
        audit = truth_audit.Audit(entry, truth, cm)
        units = score.run_units(rep)
        scope = {u["i"]: "statement" for u in score.statement_units(units)}
        scope.update({u["i"]: "factory" for u in score.factory_units(units)})
        every = score.scored_units(units)
        lab = score.label_units(truth, units, origins, audit)
        key = {(u["s"], u["e"]): u["i"] for u in every}
        claims = {key[(c["start"], c["end"])]: c for c in rep.get("claims", []) if (c["start"], c["end"]) in key}
        app = truth["appPackage"]
        for u in every:
            t = lab[u["i"]]
            if "pkg" not in t:
                continue
            c = claims.get(u["i"])
            v = score.judge_origin(c["package"], t, app)[0] if c else "unclaimed"
            fh.write(json.dumps({"entry": entry["id"], "set": entry["set"], "bytes": u["b"],
                                 "scope": scope[u["i"]], "kind": u["k"], "pkg": t["pkg"],
                                 "version": t.get("version"), "file": t.get("file"), "origin": t.get("origin"),
                                 "claimed": c["package"] if c else None, "verdict": v,
                                 "named": v in ("right", "family", "origin"),
                                 **({"w": u["w"][:2], "f": [x[2:4] for x in u["f"]]} if "w" in u and v not in (
                                     "right", "family", "origin") else {}),
                                 "slice": slice_}) + "\n")


if __name__ == "__main__":
    main()
