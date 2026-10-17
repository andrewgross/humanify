"""exp127 diagnosis: per (claimed package, truth package), the wrong-family claimed bytes of a DEV label, with the
claim's rule and evidence kind. Refuses the frozen test splits.
  $PY -I experiments/127-minifier-slices/wrongs.py <label> --corpus <dev corpus> [--set <set>] [--top N]"""
import collections
import json
import os
import sys

HERE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "096-library-matching-score")
sys.path.insert(0, HERE)
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402


def main():
    argv = lib096.corpus_flag(sys.argv)
    if lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: the frozen test splits are never diagnosed")
    label = argv[1]
    want = argv[argv.index("--set") + 1] if "--set" in argv else None
    top = int(argv[argv.index("--top") + 1]) if "--top" in argv else 25
    run_dir = os.path.join(lib096.DATA, "results", label)
    om, cm = lib096.load_origins_manifest(), truth_audit.load_copies_manifest()
    chm, o7m = truth_audit.load_chained_manifest(), truth_audit.load_origins7_manifest()
    agg = collections.Counter()
    for entry in lib096.load_manifest()["entries"]:
        if want and entry["set"] != want:
            continue
        rp = os.path.join(run_dir, "reports", entry["id"] + ".json")
        if not os.path.exists(rp):
            continue
        report = json.load(open(rp))["libraryDiscovery"]
        if report is None or entry["truth"]["method"] != "sourcemap":
            continue
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        units = score.run_units(report)
        by_i = {u["i"]: u for u in units}
        st = score.statement_units(units)
        labels = score.label_units(truth, units, origins, truth_audit.Audit(entry, truth, cm))
        key = {(u["s"], u["e"]): u["i"] for u in st}
        for c in report.get("claims", []):
            i = key.get((c["start"], c["end"]))
            if i is None:
                continue
            v, _ = score.judge_origin(c["package"], labels[i], truth["appPackage"])
            if v in ("wrongFamily", "app"):
                ev = (c.get("evidence") or {}).get("kind", "-")
                agg[(v, c["package"], labels[i]["label"], c["rule"], ev, entry["id"][:40])] += by_i[i]["b"]
    for k, b in agg.most_common(top):
        print(b, *k)
    print("total", sum(agg.values()))


if __name__ == "__main__":
    main()
