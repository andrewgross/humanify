"""exp129: the copies-v2 relabel LEDGER: what the scorer's new copies layer changed in the TRUTH JUDGEMENT, per label,
in BOTH directions.

usage: <venv python> -I ledger12.py <label>... [--corpus C] --json out.json [--rows out.jsonl]
Scorer 12 differs from scorer 11 in two places: the copies layer (exp104's frozen copies.json vs exp129's
copies2.json) and the origin layer's `patched` rule (origins12: a marker path may hold `=`). For every claimed unit
of every entry a label ran, the unit is labelled exactly as score.py labels it TWICE (both off: scorer 11; both on:
scorer 12) and the claim judged on each. A changed verdict is charged to `patched` when the origin LAYER's answer
for the unit changed, else `gained` (the copies layer gives an origin it did not, or another one) or `lost`. Classes as exp119's ledger: GOOD (right, family, origin), WRONG
(wrongFamily, app), UNJUDGED (the rest). On a private corpus (test, test2) pass --json only: per-set aggregates."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../119-families-full"))
import ledger as L119  # noqa: E402  (cls)
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402


def manifests():
    out = {}
    for on in (False, True):
        truth_audit.COPIES2["on"] = on
        out[on] = truth_audit.load_copies_manifest()
    truth_audit.COPIES2["on"] = True
    return out


def judged_units(units):
    f = getattr(score, "scored_units", score.statement_units)
    return f(units)


def main():
    argv = lib096.corpus_flag(sys.argv)
    labels = [a for a in argv[1:] if not a.startswith("--") and not a.endswith((".json", ".jsonl"))]
    jout = argv[argv.index("--json") + 1]
    rout = argv[argv.index("--rows") + 1] if "--rows" in argv else None
    if rout and lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: rows name private-corpus apps; aggregates only")
    cms = manifests()
    om, o7m, chm = (lib096.load_origins_manifest(), truth_audit.load_origins7_manifest(),
                    truth_audit.load_chained_manifest())
    pipes = {label: json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
             for label in labels}
    trans = {label: collections.Counter() for label in labels}
    rows = []
    for entry in lib096.load_manifest()["entries"]:
        if entry["truth"]["method"] != "sourcemap":
            continue
        prep = None
        for label in labels:
            if pipes[label].get("sets") and entry["set"] not in pipes[label]["sets"]:
                continue
            rp = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
            rep = json.load(open(rp))["libraryDiscovery"]
            if rep is None:
                continue
            if prep is None:
                prep = {}
                for on in (False, True):
                    import origins as O
                    O.RULES12["patched"] = on
                    truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
                    origins = truth_audit.origin_layer(entry, om, o7m)
                    truth, origins = truth_audit.redecode(entry, truth, origins)
                    prep[on] = (truth, origins, truth_audit.Audit(entry, truth, cms[on]))
                O.RULES12["patched"] = True
            units = score.run_units(rep)
            lo = score.label_units(prep[False][0], units, prep[False][1], prep[False][2])
            ln = score.label_units(prep[True][0], units, prep[True][1], prep[True][2])
            truth = prep[True][0]
            no_copy_o = score.label_units(prep[False][0], units, prep[False][1],
                                          truth_audit.Audit(entry, prep[False][0], None, ranges=[]))
            no_copy_n = score.label_units(prep[True][0], units, prep[True][1],
                                          truth_audit.Audit(entry, prep[True][0], None, ranges=[]))
            key = {(u["s"], u["e"]): u for u in judged_units(units)}
            app = truth["appPackage"]
            for c in rep.get("claims", []):
                u = key.get((c["start"], c["end"]))
                if u is None or u["i"] not in lo:
                    continue
                ro, rn = lo[u["i"]], ln[u["i"]]
                vo = score.judge_origin(c["package"], ro, app)[0]
                vn = score.judge_origin(c["package"], rn, app)[0]
                if vo == vn:
                    continue
                a, b = no_copy_o[u["i"]], no_copy_n[u["i"]]
                if (a["label"], a.get("origin")) != (b["label"], b.get("origin")):
                    kind = "patched"
                else:
                    kind = "gained" if rn.get("origin") and rn.get("origin") != ro.get("origin") else "lost"
                trans[label][(entry["set"], kind, vo, vn)] += u["b"]
                if rout:
                    rows.append({"label": label, "entry": entry["id"], "set": entry["set"], "b": u["b"],
                                 "kind": kind, "old": vo, "new": vn, "s": u["s"], "e": u["e"],
                                 "claim": c["package"], "label_": rn["label"], "oldOrigin": ro.get("origin"),
                                 "origin": rn.get("origin")})
    result = {}
    for label in labels:
        per_set = collections.defaultdict(collections.Counter)
        for (s, kind, vo, vn), b in trans[label].items():
            per_set[s][f"{L119.cls(vo)}->{L119.cls(vn)}"] += b
            per_set[s][f"{L119.cls(vo)}->{L119.cls(vn)}:{kind}"] += b
        result[label] = {"corpus": lib096.CORPUS,
                         **({} if lib096.CORPUS in lib096.PRIVATE else
                            {"transitions": [[*k, v] for k, v in sorted(trans[label].items())]}),
                         "perSet": {s: dict(sorted(c.items())) for s, c in sorted(per_set.items())}}
        print(label, json.dumps(result[label]["perSet"]), flush=True)
    open(jout, "w").write(lib096.dump(result))
    if rout:
        with open(rout, "w") as fh:
            for r in rows:
                fh.write(json.dumps(r, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
