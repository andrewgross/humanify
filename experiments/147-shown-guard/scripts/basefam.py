"""exp147: for each (bundle, family) with shown's new wrong, how main (base) already judged that family there."""
import json, os, sys, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../096-library-matching-score"))
import lib096
LAB = {"dev": "exp142-asof", "apps": "exp142-asof-apps", "bundlers": "exp142-asof-bundlers"}
for c in ("pop", "remin", "remin3", "remin4"): LAB[c] = c + "-exp142-asof"
F = json.load(open('/work/exp147-data/feat.json'))
want = collections.defaultdict(dict)
for f in F:
    nw = f['kinds'].get('newWrong', 0)
    if nw: want[f['corpus']].setdefault(f['entry'], {})[f['fam']] = (nw, f['kinds'].get('gainedRight', 0))
res = []
for corpus, ents in want.items():
    assert corpus not in lib096.PRIVATE
    lib096.use(corpus)
    import importlib, score, truth_audit
    importlib.reload(score); importlib.reload(truth_audit)
    om, cm = lib096.load_origins_manifest(), truth_audit.load_copies_manifest()
    chm, o7m = truth_audit.load_chained_manifest(), truth_audit.load_origins7_manifest()
    for entry in lib096.load_manifest()["entries"]:
        if entry["id"] not in ents: continue
        rep = json.load(open(os.path.join(lib096.DATA, "results", LAB[corpus], "reports", entry["id"] + ".json")))["libraryDiscovery"]
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        units = score.run_units(rep); lab = score.label_units(truth, units, origins, truth_audit.Audit(entry, truth, cm))
        cl = {(c["start"], c["end"]): c["package"] for c in rep["claims"]}
        agg = collections.defaultdict(collections.Counter)
        for u in score.scored_units(units):
            p = cl.get((u["s"], u["e"]))
            if p in ents[entry["id"]]:
                v = score.judge_origin(p, lab[u["i"]], truth["appPackage"])[0]
                agg[p]["right" if v in ("right", "family", "origin") else v] += u["b"]
        for p, (nw, gr) in ents[entry["id"]].items():
            res.append({"corpus": corpus, "entry": entry["id"], "fam": p, "newWrong": nw, "gainedRight": gr, "base": dict(agg[p])})
json.dump(res, open('/work/exp147-data/basefam.json', 'w'))
tot = collections.Counter()
for r in res:
    b = r["base"]; bw = b.get("wrongFamily", 0) + b.get("app", 0); br = b.get("right", 0)
    cls = "family already wrong in main (no right)" if bw and not br else "family absent in main" if not bw and not br else "family right in main" if br and not bw else "mixed"
    tot[cls] += r["newWrong"]
print({k: round(v / 1e3, 2) for k, v in tot.items()}, round(sum(tot.values()) / 1e3, 2))
