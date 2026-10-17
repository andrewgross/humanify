"""exp130: per-unit verdict diff of two labels on a DEV corpus, judged exactly as score.py (scorer on the
worktree's main). usage: jdiff.py <corpus> <out.jsonl> <labelA> <labelB>   (test/test2 refused)"""
import json, os, sys
R = "/work/exp130-sponge-origin/experiments"
sys.path.insert(0, R + "/096-library-matching-score")
import lib096
corpus, out, la, lb = sys.argv[1:5]
if corpus in ("test", "test2"): raise SystemExit("never test/test2")
lib096.use(corpus)
import score, truth_audit
om = lib096.load_origins_manifest(); cm = truth_audit.load_copies_manifest()
chm = truth_audit.load_chained_manifest(); o7m = truth_audit.load_origins7_manifest()
GOOD = ("right", "family", "origin")
fh = open(out, "w")
for entry in lib096.load_manifest()["entries"]:
    if entry["truth"]["method"] != "sourcemap": continue
    truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
    origins = truth_audit.origin_layer(entry, om, o7m)
    truth, origins = truth_audit.redecode(entry, truth, origins)
    audit = truth_audit.Audit(entry, truth, cm)
    per = {}
    for tag, label in (("a", la), ("b", lb)):
        p = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
        rep = json.load(open(p))["libraryDiscovery"]
        if rep is None: continue
        units = score.run_units(rep); st = score.statement_units(units)
        lab = score.label_units(truth, units, origins, audit)
        key = {(u["s"], u["e"]): u for u in st}
        for c in rep.get("claims", []):
            u = key.get((c["start"], c["end"]))
            if u is None: continue
            v = score.judge_origin(c["package"], lab[u["i"]], truth["appPackage"])[0]
            k = (c["start"], c["end"])
            row = per.setdefault(k, {"entry": entry["id"], "set": entry["set"], "s": k[0], "e": k[1], "bytes": u["b"],
                                     "truth": lab[u["i"]].get("pkg") or lab[u["i"]]["label"], "origin": lab[u["i"]].get("origin")})
            row[tag] = {"package": c["package"], "rule": c["rule"], "round": c["round"],
                        "evidence": (c.get("evidence") or {}).get("kind", ""), "verdict": v}
    for r in per.values():
        va = r.get("a", {}).get("verdict"); vb = r.get("b", {}).get("verdict")
        if va == vb and r.get("a", {}).get("package") == r.get("b", {}).get("package"): continue
        fh.write(json.dumps(r) + "\n")
