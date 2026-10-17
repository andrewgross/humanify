"""exp128: the factory scope's wrong-family claims of one DEV label, grouped by (claimed, truth label, origin,
rule) — the diagnosis table (dev corpora only: never run on test / test2).

usage: <venv python> -I wrong.py <label> --corpus C [--sets A,B] [--top N]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402


def main():
    argv = lib096.corpus_flag(sys.argv)
    if lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: a frozen judge is never diagnosed")
    label = argv[1]
    sets = argv[argv.index("--sets") + 1].split(",") if "--sets" in argv else None
    top = int(argv[argv.index("--top") + 1]) if "--top" in argv else 40
    run_dir = os.path.join(lib096.DATA, "results", label)
    man = lib096.load_manifest()
    om, o7m = lib096.load_origins_manifest(), truth_audit.load_origins7_manifest()
    chm, cm = truth_audit.load_chained_manifest(), truth_audit.load_copies_manifest()
    groups, per_app = collections.Counter(), collections.Counter()
    for entry in man["entries"]:
        if sets and entry["set"] not in sets:
            continue
        doc = json.load(open(os.path.join(run_dir, "reports", entry["id"] + ".json")))
        rep = doc["libraryDiscovery"]
        if rep is None or entry["truth"]["method"] != "sourcemap":
            continue
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        units = score.run_units(rep)
        fac = {u["i"]: u for u in score.factory_units(units)}
        labels = score.label_units(truth, units, origins, truth_audit.Audit(entry, truth, cm))
        key = {(u["s"], u["e"]): i for i, u in fac.items()}
        for c in rep["claims"]:
            i = key.get((c["start"], c["end"]))
            if i is None:
                continue
            v, _ = score.judge_origin(c["package"], labels[i], truth["appPackage"])
            if v in ("wrongFamily", "app"):
                t = labels[i]
                groups[(v, c["package"], t["label"], t.get("origin", "-"), c["rule"])] += fac[i]["b"]
                per_app[entry["id"]] += fac[i]["b"]
    total = sum(groups.values())
    print(f"wrong + app bytes in the factory scope: {total}")
    for (v, cl, lab, orig, rule), b in groups.most_common(top):
        print(f"{b:10d} {v:11s} claimed {cl:32s} truth {lab:32s} origin {orig:20s} {rule}")
    print("\nper bundle:")
    for e, b in per_app.most_common(15):
        print(f"{b:10d} {e}")


if __name__ == "__main__":
    main()
