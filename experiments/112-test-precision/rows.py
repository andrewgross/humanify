"""exp112: one row per claimed statement unit of a label, judged exactly as exp096 score.py (scorer 6) judges it
(patch_sources, redecode, origins, the truth audit, derived families), for any corpus.

usage: <venv python> -I rows.py <corpus> <label> <out.jsonl> [--sets a,b] [--s6]   (--s6: scorer 7's rules off)
Rows carry the entry id and the truth file: for the TEST corpus they stay under /work (never committed); only
aggregates computed from them are reported."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402

lib096.use(sys.argv[1])
import origins  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402

if "--s6" in sys.argv:
    for k in origins.RULES7:
        origins.RULES7[k] = False


def entry_rows(label, entry, om, cm, chm, o7m):
    doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")))
    rep = doc["libraryDiscovery"]
    if rep is None:
        return []
    truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
    origins = truth_audit.origin_layer(entry, om, o7m)
    truth, origins = truth_audit.redecode(entry, truth, origins)
    units = score.run_units(rep)
    st = score.statement_units(units)
    labels = score.label_units(truth, units, origins, truth_audit.Audit(entry, truth, cm))
    by_i = {u["i"]: u for u in st}
    key = {(u["s"], u["e"]): u["i"] for u in st}
    app = truth["appPackage"]
    out = []
    for c in rep.get("claims", []):
        i = key.get((c["start"], c["end"]))
        if i is None:
            continue
        lab = labels[i]
        v = score.judge_origin(c["package"], lab, app)[0]
        ev = c.get("evidence") or {}
        out.append({"entry": entry["id"], "set": entry["set"], "kind": entry.get("kind"), "start": c["start"],
                    "end": c["end"], "bytes": by_i[i]["b"], "package": c["package"],
                    "claimVersion": c.get("version"), "rule": c["rule"], "round": c["round"],
                    "evidence": ev.get("kind", ""), "ev": ev, "verdict": v, "truth": lab["label"],
                    "truthVersion": lab.get("version"), "truthFile": lab.get("file"), "origin": lab.get("origin"),
                    "originEvidence": lab.get("originEvidence"), "app": app})
    return out


def main():
    corpus, label, out = sys.argv[1:4]
    sets = set(sys.argv[sys.argv.index("--sets") + 1].split(",")) if "--sets" in sys.argv else None
    om, cm, chm = lib096.load_origins_manifest(), truth_audit.load_copies_manifest(), \
        truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    with open(out, "w") as fh:
        for e in lib096.load_manifest()["entries"]:
            if (sets and e["set"] not in sets) or e["truth"]["method"] != "sourcemap":
                continue
            if not os.path.exists(os.path.join(lib096.DATA, "results", label, "reports", e["id"] + ".json")):
                continue
            for r in entry_rows(label, e, om, cm, chm, o7m):
                fh.write(json.dumps(r) + "\n")


if __name__ == "__main__":
    main()
