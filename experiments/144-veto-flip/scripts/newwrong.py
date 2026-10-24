"""exp144: every unit whose verdict changed between a base and a new label (scorer 14), one row per unit: newly
wrong (wrongFamily / app), newly right, lost right. Rows name bundles -> written OUTSIDE the repo. Dev corpora only.
usage: python3 -I newwrong.py <out.json> <corpus>=<base label>:<new label> ..."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402

RIGHT = ("right", "family", "origin")


def main():
    out, rows = sys.argv[1], []
    for arg in sys.argv[2:]:
        corpus, labs = arg.split("=", 1)
        base, new = labs.split(":")
        if corpus in lib096.PRIVATE:
            raise SystemExit("REFUSED: never diagnose a frozen judge")
        lib096.use(corpus)
        import score
        import truth_audit
        om, cm = lib096.load_origins_manifest(), truth_audit.load_copies_manifest()
        chm, o7m = truth_audit.load_chained_manifest(), truth_audit.load_origins7_manifest()
        for entry in lib096.load_manifest()["entries"]:
            if entry["truth"]["method"] != "sourcemap":
                continue
            reps = []
            for lab in (base, new):
                p = os.path.join(lib096.DATA, "results", lab, "reports", entry["id"] + ".json")
                reps.append(json.load(open(p))["libraryDiscovery"] if os.path.exists(p) else None)
            if None in reps:
                continue
            c0 = {(c["start"], c["end"]): c for c in reps[0].get("claims", [])}
            c1 = {(c["start"], c["end"]): c for c in reps[1].get("claims", [])}
            changed = {k for k in set(c0) | set(c1)
                       if (c0.get(k) or {}).get("package") != (c1.get(k) or {}).get("package")}
            if not changed:
                continue
            truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
            origins = truth_audit.origin_layer(entry, om, o7m)
            truth, origins = truth_audit.redecode(entry, truth, origins)
            audit = truth_audit.Audit(entry, truth, cm)
            units = score.run_units(reps[1])
            lab = score.label_units(truth, units, origins, audit)
            app = truth["appPackage"]
            for u in score.scored_units(units):
                k = (u["s"], u["e"])
                if k not in changed:
                    continue
                t = lab[u["i"]]
                v = [score.judge_origin(c[k]["package"], t, app)[0] if k in c else None for c in (c0, c1)]
                ok = [x in RIGHT if x else False for x in v]
                bad = [x in ("wrongFamily", "app") for x in v]
                kind = ("newWrong" if bad[1] and not bad[0] else "gainedRight" if ok[1] and not ok[0]
                        else "lostRight" if ok[0] and not ok[1] else "fixedWrong" if bad[0] and not bad[1] else "other")
                rows.append({"corpus": corpus, "set": entry["set"], "entry": entry["id"], "s": k[0], "e": k[1],
                             "bytes": u["b"], "kind": kind, "verdict": v,
                             "base": (c0.get(k) or {}).get("package"), "new": (c1.get(k) or {}).get("package"),
                             "rule": (c1.get(k) or c0.get(k))["rule"], "truth": t.get("pkg", t["label"])})
    json.dump(rows, open(out, "w"))
    import collections
    agg = collections.defaultdict(collections.Counter)
    for r in rows:
        agg[r["set"]][r["kind"]] += r["bytes"]
    for s, c in sorted(agg.items()):
        print(s, {k: round(v / 1024, 1) for k, v in c.items()})


if __name__ == "__main__":
    main()
