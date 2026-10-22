"""exp133: per-claim verdicts EXACTLY as scorer 13's score_map judges them (both scopes: statements AND cut factory
bodies / whole factories; chained + origin7 + redecode + copies Audit incl. misattributed), for one or two labels on
a DEV corpus. exp125's judge.py is scorer 9 (statement units only). Output stays OUTSIDE the repo (it names
packages). test / test2 refused.

usage: python -I judge13.py <corpus> <out.jsonl> <label> [<label2>]
Each row: a unit claimed by either label: entry, set, span, bytes, scope, truth label/origin, and per label
c0/c1 = {package, rule, round, evidence, verdict}. Sum `bytes` by c1.verdict == wrongFamily to reproduce the card's
wrongFamilyBytes."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402


def main():
    corpus, out, labels = sys.argv[1], sys.argv[2], sys.argv[3:]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never diagnose a test split")
    lib096.use(corpus)
    import score
    import truth_audit
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    fh = open(out, "w")
    for entry in lib096.load_manifest()["entries"]:
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        if truth["method"] != "sourcemap":
            continue
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        audit = truth_audit.Audit(entry, truth, cm)
        per = {}
        for li, label in enumerate(labels):
            p = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
            if not os.path.exists(p):
                continue
            rep = json.load(open(p))["libraryDiscovery"]
            if rep is None:
                continue
            units = score.run_units(rep)
            scope = {u["i"]: "statement" for u in score.statement_units(units)}
            scope.update({u["i"]: "factory" for u in score.factory_units(units)})
            every = score.scored_units(units)
            lab = score.label_units(truth, units, origins, audit)
            key = {(u["s"], u["e"]): u for u in every}
            for c in rep.get("claims", []):
                u = key.get((c["start"], c["end"]))
                if u is None:
                    continue
                v = score.judge_origin(c["package"], lab[u["i"]], truth["appPackage"])[0]
                k = (c["start"], c["end"])
                row = per.setdefault(k, {"entry": entry["id"], "set": entry["set"], "start": k[0], "end": k[1],
                                         "bytes": u["b"], "scope": scope[u["i"]], "kind": u["k"],
                                         "module": u.get("m"), "app": truth["appPackage"],
                                         "truth": lab[u["i"]]["label"], "origin": lab[u["i"]].get("origin")})
                row["c%d" % li] = {"package": c["package"], "rule": c["rule"], "round": c["round"],
                                   "evidence": (c.get("evidence") or {}).get("kind", ""), "verdict": v}
        for r in per.values():
            fh.write(json.dumps(r) + "\n")


if __name__ == "__main__":
    main()
