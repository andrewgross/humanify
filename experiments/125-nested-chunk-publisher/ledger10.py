"""exp125: the scorer 9 -> 10 relabel LEDGER, per label, in BOTH directions.

usage: <venv python> -I ledger10.py --corpus C --json out.json [--rows out.jsonl] <label>...
Scorer 10 differs from scorer 9 in one place: a unit whose origin is a NESTED chunk carries the region's package
(`via`, origins.py RULES10) and judge_origin accepts a claim naming its family. Every claimed statement unit of
every entry a label ran is labelled exactly as score.py labels it (scorer 10's layer), and the claim is judged
twice: without `via` (scorer 9's verdict: origins10.py verify proves the layer otherwise equal to scorer 7's) and
with it. Classes: GOOD (right, family, origin), WRONG (wrongFamily, app), UNJUDGED (the rest). On a private corpus
(test, test2) pass --json only: per-set aggregates."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv[1:])
import score  # noqa: E402
import truth_audit  # noqa: E402

GOOD = ("right", "family", "origin")


def cls(v):
    return "GOOD" if v in GOOD else "WRONG" if v in ("wrongFamily", "app") else "UNJUDGED"


def opt(name):
    if name not in ARGV:
        return None
    i = ARGV.index(name)
    v = ARGV[i + 1]
    del ARGV[i:i + 2]
    return v


def main():
    out_json, out_rows = opt("--json"), opt("--rows")
    if out_rows and lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("private corpus: --json aggregates only")
    labels = ARGV
    om, cm = lib096.load_origins_manifest(), truth_audit.load_copies_manifest()
    chm, o7m = truth_audit.load_chained_manifest(), truth_audit.load_origins7_manifest()
    led = {lb: collections.Counter() for lb in labels}
    rows = open(out_rows, "w") if out_rows else None
    for entry in lib096.load_manifest()["entries"]:
        if entry["truth"]["method"] != "sourcemap":
            continue
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        audit = truth_audit.Audit(entry, truth, cm)
        for lb in labels:
            p = os.path.join(lib096.DATA, "results", lb, "reports", entry["id"] + ".json")
            if not os.path.exists(p):
                continue
            rep = json.load(open(p))["libraryDiscovery"]
            if rep is None:
                continue
            units = score.run_units(rep)
            labs = score.label_units(truth, units, origins, audit)
            key = {(u["s"], u["e"]): u for u in score.statement_units(units)}
            for c in rep.get("claims", []):
                u = key.get((c["start"], c["end"]))
                if u is None:
                    continue
                row = labs[u["i"]]
                on = score.judge_origin(c["package"], row, truth["appPackage"])[0]
                off = score.judge_origin(c["package"], {k: v for k, v in row.items() if k != "via"},
                                         truth["appPackage"])[0]
                if on == off:
                    continue
                led[lb][(entry["set"], f"{cls(off)}->{cls(on)}")] += u["b"]
                if rows:
                    rows.write(json.dumps({"label": lb, "entry": entry["id"], "start": c["start"], "end": c["end"],
                                           "bytes": u["b"], "package": c["package"], "truth": row["label"],
                                           "origin": row.get("origin"), "via": row.get("via"), "off": off,
                                           "on": on, "rule": c["rule"]}) + "\n")
    res = {lb: {f"{s} {k}": v for (s, k), v in sorted(c.items())} for lb, c in led.items()}
    for lb, c in res.items():
        print(lb, c or "no verdict moved")
    if out_json:
        open(out_json, "w").write(lib096.dump({"schema": "exp125-ledger10/1", "corpus": lib096.CORPUS,
                                               "labels": res}))


if __name__ == "__main__":
    main()
