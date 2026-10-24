"""exp143: what an ablation label claims that the base label does not (and vice versa), judged by scorer 14: per
set KB of newly RIGHT / newly WRONG (wrongFamily + app) bytes, broken down by the new claim's rule, the unit's
whole-print node bucket and bytes per node; and the top wrong (claimed package, truth package) pairs by KB.
Dev corpora only. usage: <venv python> -I claimdiff.py <out.json> <corpus>=<base label>:<new label> ..."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402

RIGHT = ("right", "family", "origin")


def bucket(u):
    if "w" not in u:
        return "no print"
    n = u["w"][1]
    b = "<20" if n < 20 else "20-29" if n < 30 else "30-99" if n < 100 else ">=100"
    return b + (" literal-heavy" if u["b"] >= 16 * max(n, 1) else "")


def main():
    out = sys.argv[1]
    res = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    pairs = collections.defaultdict(collections.Counter)
    for arg in sys.argv[2:]:
        corpus, labs = arg.split("=", 1)
        base, new = labs.split(":")
        if corpus in lib096.PRIVATE:
            raise SystemExit("REFUSED: never diagnose a frozen judge")
        lib096.use(corpus)
        import score
        import truth_audit
        om = lib096.load_origins_manifest()
        cm = truth_audit.load_copies_manifest()
        chm = truth_audit.load_chained_manifest()
        o7m = truth_audit.load_origins7_manifest()
        for entry in lib096.load_manifest()["entries"]:
            if entry["truth"]["method"] != "sourcemap":
                continue
            reps = []
            for lab in (base, new):
                p = os.path.join(lib096.DATA, "results", lab, "reports", entry["id"] + ".json")
                reps.append(json.load(open(p))["libraryDiscovery"] if os.path.exists(p) else None)
            if reps[0] is None or reps[1] is None:
                continue
            c0 = {(c["start"], c["end"]): c for c in reps[0].get("claims", [])}
            c1 = {(c["start"], c["end"]): c for c in reps[1].get("claims", [])}
            changed = {k for k in set(c0) | set(c1) if (c0.get(k) or {}).get("package") != (c1.get(k) or {}).get(
                "package")}
            if not changed:
                continue
            truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
            origins = truth_audit.origin_layer(entry, om, o7m)
            truth, origins = truth_audit.redecode(entry, truth, origins)
            audit = truth_audit.Audit(entry, truth, cm)
            units = score.run_units(reps[1])
            scored = score.scored_units(units)
            lab = score.label_units(truth, units, origins, audit)
            app = truth["appPackage"]
            for u in scored:
                k = (u["s"], u["e"])
                if k not in changed:
                    continue
                t = lab[u["i"]]
                v = [score.judge_origin(c[k]["package"], t, app)[0] if k in c else None for c in (c0, c1)]
                ok = [x in RIGHT if x else False for x in v]
                bad = [x in ("wrongFamily", "app") for x in v]
                key = (c1.get(k) or c0.get(k))["rule"] + " | " + bucket(u)
                s = entry["set"]
                if ok[1] and not ok[0]:
                    res[s]["gainedRight"][key] += u["b"]
                if ok[0] and not ok[1]:
                    res[s]["lostRight"][key] += u["b"]
                if bad[1] and not bad[0]:
                    res[s]["newWrong"][key] += u["b"]
                    pairs[s][(c1[k]["package"], t.get("pkg", t["label"]))] += u["b"]
                if bad[0] and not bad[1]:
                    res[s]["fixedWrong"][key] += u["b"]
    kb = lambda b: round(b / 1024, 1)  # noqa: E731
    doc = {s: {**{w: {"total": kb(sum(c.values())), **{k: kb(v) for k, v in c.most_common(8)}}
                  for w, c in d.items()},
               **({"topWrong": [[f"{a} <- {b}", kb(v)] for (a, b), v in pairs[s].most_common(6)]}
                  if s != "heldout" else {})}
           for s, d in sorted(res.items())}
    json.dump(doc, open(out, "w"), indent=1)
    for s, d in doc.items():
        print(s, {w: d[w]["total"] for w in ("gainedRight", "lostRight", "newWrong", "fixedWrong") if w in d})
        if "newWrong" in d:
            print("   newWrong:", {k: v for k, v in d["newWrong"].items() if k != "total"})
        if "topWrong" in d:
            print("   top:", d["topWrong"][:4])


if __name__ == "__main__":
    main()
