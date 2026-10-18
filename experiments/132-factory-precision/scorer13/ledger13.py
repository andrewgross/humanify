"""exp132 scorer 13, rule `misattributed` (truth_audit.Audit.misattributed): the BOTH-DIRECTIONS ledger of one
label — every unit the rule relabels, and how each claim's verdict moves (rule off -> on).

usage: <venv python> -I ledger13.py <label> --corpus C [--draw N --seed S]   (prints aggregates; --draw prints N
       relabelled units for the hand-check, dev corpora only)

Totals per set: relabelled units / bytes (to a package, to <unmapped>), and claimed bytes by (verdict off ->
verdict on). Read from the frozen truth and the label's reports only."""
import collections
import json
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402


def main():
    argv = lib096.corpus_flag(sys.argv)
    label = argv[1]
    draw = int(argv[argv.index("--draw") + 1]) if "--draw" in argv else 0
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else 132
    if draw and lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: a frozen judge is never hand-checked")
    run_dir = os.path.join(lib096.DATA, "results", label)
    man = lib096.load_manifest()
    om, o7m = lib096.load_origins_manifest(), truth_audit.load_origins7_manifest()
    chm, cm = truth_audit.load_chained_manifest(), truth_audit.load_copies_manifest()
    agg = collections.defaultdict(collections.Counter)
    moves = collections.defaultdict(collections.Counter)
    pool = []
    for entry in man["entries"]:
        if entry["truth"]["method"] != "sourcemap":
            continue
        p = os.path.join(run_dir, "reports", entry["id"] + ".json")
        rep = json.load(open(p))["libraryDiscovery"]
        if rep is None:
            continue
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        units = score.run_units(rep)
        audit = truth_audit.Audit(entry, truth, cm)
        on = score.label_units(truth, units, origins, audit)
        mis = audit.misattributed(units)
        layer, audit.mis13 = audit.mis13, {}
        off = score.label_units(truth, units, origins, audit)
        audit.mis13 = layer
        claims = {(c["start"], c["end"]): c for c in rep.get("claims", [])}
        a, mv = agg[entry["set"]], moves[entry["set"]]
        a["factories judged misattributed"] += len(mis)
        for row in mis.values():
            a["factories by evidence: " + row[6]] += 1
            pool.append((entry["id"], row[0], row[1], row[1] - row[0], row[2], row[3], row[5], row[6]))
        for u in score.scored_units(units):
            t1, t0 = on[u["i"]], off[u["i"]]
            if "misattributedFrom" not in t1:
                continue
            a["units"] += 1
            a["bytes"] += u["b"]
            a["bytes -> <unmapped>" if t1["label"] == "<unmapped>" else "bytes -> a package"] += u["b"]
            c = claims.get((u["s"], u["e"]))
            if c:
                v0 = score.judge_origin(c["package"], t0, truth["appPackage"])[0]
                v1 = score.judge_origin(c["package"], t1, truth["appPackage"])[0]
                mv[f"{v0} -> {v1}"] += u["b"]
                if draw and v0 in ("right", "family", "origin") and v1 in ("wrongFamily", "app"):
                    print(f"  right->wrong {entry['id']} @{u['s']} {u['b']} B claim {c['package']} | old {t0['label']} "
                          f"{t0.get('file')} | new {t1['label']} {t1.get('file')}")
    for s in sorted(agg):
        print(s, dict(agg[s]))
        for k, v in moves[s].most_common():
            print(f"    {v:>12,}  {k}")
    if draw:
        rnd = random.Random(seed)
        pool.sort()
        for x in rnd.sample(pool, min(draw, len(pool))):
            entry = next(e for e in man["entries"] if e["id"] == x[0])
            text = open(lib096.corpus_path(entry, "bundle"), "rb").read()[x[1]:x[2]]
            print("\n==", x[0], f"@{x[1]}", f"{x[3]} B", "| old", x[4], "| new", x[5], x[6], "|", x[7])
            print("  ", text[:300].decode("utf8", "replace").replace("\n", " "))


if __name__ == "__main__":
    main()
