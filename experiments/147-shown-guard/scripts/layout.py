"""exp147: per-bundle layout dump around a family's rescued seeds (dev only, output stays in /work).
usage: layout.py <corpus> <base> <new> <entry-substring> <family-prefix>"""
import json, os, sys, collections, bisect
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../096-library-matching-score"))
import lib096
corpus, base, new, sub, fam = sys.argv[1:6]
assert corpus not in lib096.PRIVATE
lib096.use(corpus)
import score, truth_audit
om, cm = lib096.load_origins_manifest(), truth_audit.load_copies_manifest()
chm, o7m = truth_audit.load_chained_manifest(), truth_audit.load_origins7_manifest()
for entry in lib096.load_manifest()["entries"]:
    if sub not in entry["id"]: continue
    reps = [json.load(open(os.path.join(lib096.DATA, "results", l, "reports", entry["id"] + ".json")))["libraryDiscovery"] for l in (base, new)]
    truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
    origins = truth_audit.origin_layer(entry, om, o7m)
    truth, origins = truth_audit.redecode(entry, truth, origins)
    units = score.run_units(reps[1]); lab = score.label_units(truth, units, origins, truth_audit.Audit(entry, truth, cm))
    tl = {(u["s"], u["e"]): lab[u["i"]].get("pkg", lab[u["i"]]["label"]) for u in score.scored_units(units)}
    c0 = {(c["start"], c["end"]): c for c in reps[0]["claims"]}; c1 = {(c["start"], c["end"]): c for c in reps[1]["claims"]}
    vet0 = {(v["start"], v["end"]): v["package"] for v in reps[0]["vetoed"]}
    rows = reps[1]["unitRows"]
    # per module: truth majority, base claims, new claims
    mods = collections.OrderedDict()
    for r in rows:
        m = r.get("module", -1); k = (r["start"], r["end"])
        d = mods.setdefault(m, {"b": 0, "truth": collections.Counter(), "base": collections.Counter(), "new": collections.Counter(), "seed0": collections.Counter(), "seed1": collections.Counter(), "vet": collections.Counter()})
        d["b"] += r["bytes"]; d["truth"][tl.get(k, "?")] += r["bytes"]
        if k in c0: d["base"][c0[k]["package"]] += r["bytes"]; 
        if k in c0 and c0[k]["rule"] == "seed": d["seed0"][c0[k]["package"]] += 1
        if k in c1: d["new"][c1[k]["package"]] += r["bytes"]
        if k in c1 and c1[k]["rule"] == "seed": d["seed1"][c1[k]["package"]] += 1
        if k in vet0: d["vet"][vet0[k]] += 1
    keys = list(mods)
    hit = [i for i, m in enumerate(keys) if any(p.startswith(fam) for p in mods[m]["new"]) or any(p.startswith(fam) for p in mods[m]["base"])]
    show = sorted({j for i in hit for j in range(i - 2, i + 3) if 0 <= j < len(keys)})
    print(entry["id"], "app", truth["appPackage"])
    last = None
    for j in show:
        if last is not None and j != last + 1: print("   ...")
        last = j; m = keys[j]; d = mods[m]
        f = lambda c: ",".join(f"{p}:{v}" for p, v in c.most_common(3))
        print(f"{j:6d} m{m} {d['b']:6d}B truth[{f(d['truth'])}] base[{f(d['base'])}] new[{f(d['new'])}] s0[{f(d['seed0'])}] s1[{f(d['seed1'])}] vet[{f(d['vet'])}]")
