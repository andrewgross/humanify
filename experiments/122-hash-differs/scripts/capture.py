"""exp122 step 1a: every library unit of one scored DEV label, with its miss class, written to a PRIVATE file
outside the repo (/work/exp122-data/units-<label>.jsonl.gz). Prints aggregates only.

Re-runs exp096's scorer (score.py, unchanged) on the label's reports with decompose.classify wrapped, so every
statement unit keeps (truth label, package, version, file, verdict/miss class, span, bytes, whole print). The
private rows are read by the lab tools (diff.py, simulate.py) for aggregate shares; nothing here prints or
commits a package name, and no package list is chosen from them.

usage: <exp095 scale venv python> -I capture.py <label> [--corpus dev|apps|pop|bundlers]
"""
import collections
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402
import decompose  # noqa: E402

OUT = "/work/exp122-data"


def main():
    argv = lib096.corpus_flag(sys.argv)
    label = argv[1]
    if lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: test/test2 are judges, never diagnosed")
    run_dir = os.path.join(lib096.DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    captured = {}
    orig = decompose.classify

    def wrapped(ix, u, t, claim, vetoed, struct_claims, rule):
        c = orig(ix, u, t, claim, vetoed, struct_claims, rule)
        captured[u["i"]] = c
        return c
    decompose.classify = wrapped
    stash = {}
    orig_label = score.label_units

    def wrapped_label(truth, units, origins=None, audit=None):
        r = orig_label(truth, units, origins, audit)
        stash["labels"] = r
        stash["units"] = units
        return r
    score.label_units = wrapped_label
    indexes = {}
    tot = collections.defaultdict(collections.Counter)
    out = gzip.open(os.path.join(OUT, f"units-{label}.jsonl.gz"), "wt")
    for entry in lib096.load_manifest()["entries"]:
        if pipeline.get("sets") and entry["set"] not in pipeline["sets"]:
            continue
        if entry["truth"]["method"] != "sourcemap":
            continue  # Claude Code: sample truth, no file (d-nofile): out of this census
        doc = json.load(open(os.path.join(run_dir, "reports", entry["id"] + ".json")))
        report = doc["libraryDiscovery"]
        if report is None:
            continue
        slice_ = doc["toolchain"]["slice"]
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        if slice_ not in indexes:
            indexes[slice_] = score.load_index(pipeline["indexDir"], slice_, pipeline["indexId"])
        captured.clear()
        stash.clear()
        score.score_map(entry, report, truth, indexes[slice_], pipeline["rule"], origins,
                        truth_audit.Audit(entry, truth, cm))
        labels, units = stash["labels"], stash["units"]
        by_i = {u["i"]: u for u in units}
        claims = {(c["start"], c["end"]): c["package"] for c in report.get("claims", [])}
        rows = []
        for i, t in labels.items():
            u = by_i[i]
            lib = "pkg" in t
            cls = captured.get(i, "named" if lib else t["label"])
            if lib and "w" not in u and i not in captured:
                cls = "unit-model"
            tot[entry["set"]][cls] += u["b"]
            rows.append({"i": i, "s": u["s"], "e": u["e"], "b": u["b"], "label": t["label"], "pkg": t.get("pkg"),
                         "v": t.get("version"), "file": t.get("file"), "origin": t.get("origin"), "cls": cls,
                         "claim": claims.get((u["s"], u["e"])), "w": u.get("w")})
        out.write(json.dumps({"entry": entry["id"], "set": entry["set"], "slice": slice_,
                              "bundle": lib096.corpus_path(entry, "bundle"), "units": rows}) + "\n")
    out.close()
    decompose.classify = orig
    for s, c in sorted(tot.items()):
        lib = sum(v for k, v in c.items() if not k.startswith("<"))
        d = {k: round(v / lib, 5) for k, v in sorted(c.items()) if k.startswith("d")}
        print(s, "libraryBytes", lib, "d-shares", d, "d total", round(sum(d.values()), 5))


if __name__ == "__main__":
    main()
