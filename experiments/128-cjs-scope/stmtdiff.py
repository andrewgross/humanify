"""exp128: what factory_bodies changed in the STATEMENT scope between two DEV labels on the same corpus — bytes per
(verdict before -> verdict after, package before -> after), the top groups. Dev corpora only.

usage: <venv python> -I stmtdiff.py <base label> <label> --corpus C [--sets A,B] [--top N]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402


def verdicts(rep, labels, app_pkg):
    units = score.run_units(rep)
    st = {(u["s"], u["e"]): u for u in score.statement_units(units)}
    out = {}
    for c in rep["claims"]:
        u = st.get((c["start"], c["end"]))
        if u is not None:
            out[(u["s"], u["e"])] = (c["package"], score.judge_origin(c["package"], labels[u["i"]], app_pkg)[0])
    return st, out


def main():
    argv = lib096.corpus_flag(sys.argv)
    if lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: a frozen judge is never diagnosed")
    base, label = argv[1], argv[2]
    sets = argv[argv.index("--sets") + 1].split(",") if "--sets" in argv else None
    top = int(argv[argv.index("--top") + 1]) if "--top" in argv else 30
    om, o7m = lib096.load_origins_manifest(), truth_audit.load_origins7_manifest()
    chm, cm = truth_audit.load_chained_manifest(), truth_audit.load_copies_manifest()
    groups = collections.Counter()
    for entry in lib096.load_manifest()["entries"]:
        if (sets and entry["set"] not in sets) or entry["truth"]["method"] != "sourcemap":
            continue
        reps = [json.load(open(os.path.join(lib096.DATA, "results", lab, "reports", entry["id"] + ".json")))
                ["libraryDiscovery"] for lab in (base, label)]
        if reps[0] is None:
            continue
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        audit = truth_audit.Audit(entry, truth, cm)
        (st, a), (_, b) = [verdicts(r, score.label_units(truth, score.run_units(r), origins, audit),
                                     truth["appPackage"]) for r in reps]
        for k in set(a) | set(b):
            pa, va = a.get(k, ("-", "unclaimed"))
            pb, vb = b.get(k, ("-", "unclaimed"))
            if (pa, va) != (pb, vb):
                groups[(va, vb, pa, pb)] += st[k]["b"]
    by_t = collections.Counter()
    for (va, vb, _, _), n in groups.items():
        by_t[(va, vb)] += n
    for (va, vb), n in by_t.most_common():
        print(f"{n:10d} {va} -> {vb}")
    print()
    for (va, vb, pa, pb), n in groups.most_common(top):
        print(f"{n:10d} {va:11s} -> {vb:11s} {pa:30s} -> {pb}")


if __name__ == "__main__":
    main()
