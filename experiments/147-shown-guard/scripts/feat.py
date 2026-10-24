"""exp147: per (bundle, family) of shown's changed units: changed bytes by verdict and by module, rescued seeds,
and candidate guard features. Dev only; rows name bundles -> /work only.
usage: feat.py <out.json> <corpus>=<base>:<new> ..."""
import bisect, collections, json, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../096-library-matching-score"))
import lib096

def load(corpus, lab, eid):
    p = os.path.join(lib096.CORPORA[corpus]["data"], "results", lab, "reports", eid + ".json")
    return json.load(open(p))["libraryDiscovery"] if os.path.exists(p) else None

def main():
    out = []
    for arg in sys.argv[2:]:
        corpus, labs = arg.split("=", 1); base, new = labs.split(":")
        assert corpus not in lib096.PRIVATE
        nw = json.load(open(f"/work/exp147-data/nw-{corpus}.json"))
        per = collections.defaultdict(list)
        for r in nw:
            per[r["entry"]].append(r)
        for eid, rs in sorted(per.items()):
            b, n = load(corpus, base, eid), load(corpus, new, eid)
            if not b or not n:
                continue
            rows = n["unitRows"]
            mod = {(r["start"], r["end"]): r.get("module", -1) for r in rows}
            mbytes = collections.Counter()
            for r in rows: mbytes[r.get("module", -1)] += r["bytes"]
            c0 = {(c["start"], c["end"]): c for c in b["claims"]}
            c1 = {(c["start"], c["end"]): c for c in n["claims"]}
            fam_mod0 = collections.defaultdict(collections.Counter)   # base claimed bytes per family per module
            fam_mod1 = collections.defaultdict(collections.Counter)
            for r in rows:
                k = (r["start"], r["end"]); m = r.get("module", -1)
                if k in c0: fam_mod0[c0[k]["package"]][m] += r["bytes"]
                if k in c1: fam_mod1[c1[k]["package"]][m] += r["bytes"]
            seeds0 = collections.defaultdict(list)
            for k, c in c0.items():
                if c["rule"] == "seed": seeds0[c["package"]].append((k, (c.get("evidence") or {}).get("kind")))
            fams = collections.defaultdict(lambda: {"kinds": collections.Counter(), "mods": collections.defaultdict(collections.Counter)})
            for r in rs:
                p = r["new"] or r["base"]
                if not p: continue
                k = (r["s"], r["e"])
                fams[p]["kinds"][r["kind"]] += r["bytes"]
                fams[p]["mods"][mod.get(k, -1)][r["kind"]] += r["bytes"]
            for p, d in fams.items():
                rescued = [(k, mod.get(k, -1), c["end"] - c["start"]) for k, c in c1.items()
                           if c["package"] == p and c["rule"] == "seed" and not (k in c0 and c0[k]["rule"] == "seed")]
                kept = seeds0.get(p, [])
                kept_mods = {mod.get(k, -1) for k, _ in kept}
                full0 = [m for m in fam_mod0[p] if m != -1 and fam_mod0[p][m] >= mbytes[m]]
                scope = p.split("/")[0] if p.startswith("@") else p
                famp = [q for q in fam_mod0 if (q.split("/")[0] if q.startswith("@") else q) == scope]
                seedmods = {mod.get(k, -1) for q in famp for k, e in seeds0.get(q, [])}
                fullseed = [m for m in seedmods if m != -1 and sum(fam_mod0[q][m] for q in famp) >= mbytes[m]]
                anyseed = sum(len(seeds0.get(q, [])) for q in famp)
                decl = sum(1 for q in famp for _, e in seeds0.get(q, []) if e in ("declared", "banner"))
                # coverage of the kept seeds' modules by the family in base
                kept_cov = sum(fam_mod0[p][m] for m in kept_mods) / max(1, sum(mbytes[m] for m in kept_mods))
                out.append({"corpus": corpus, "entry": eid, "fam": p,
                            "kinds": dict(d["kinds"]), "mods": {str(m): dict(v) for m, v in d["mods"].items()},
                            "rescued": [{"mod": m, "bytes": by, "modBytes": mbytes[m],
                                         "modCov1": fam_mod1[p][m] / max(1, mbytes[m]),
                                         "modCov0": fam_mod0[p][m] / max(1, mbytes[m])} for _, m, by in rescued],
                            "kept": len(kept), "keptDeclared": sum(1 for _, e in kept if e != "index"),
                            "keptBytes": sum(k[1] - k[0] for k, _ in kept),
                            "fullMods0": len(full0), "famBytes0": sum(fam_mod0[p].values()),
                            "keptCov": kept_cov, "fullSeedMods": len(fullseed), "scopeKept": anyseed, "scopeDecl": decl})
    json.dump(out, open(sys.argv[1], "w"))
    print(len(out))

main()
