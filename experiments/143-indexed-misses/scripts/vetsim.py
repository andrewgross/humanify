"""exp143: what the isolation veto drops, judged. For every index seed a run's report lists as VETOED (the unit and
the package it would have claimed), the scorer-14 verdict the claim would get (right / family / origin = right;
wrongFamily / app / selfOther = wrong), with the features a veto exemption could read: the unit's share of its
module group (a whole-module body), its kind, and whether the index is ambiguous about it (another family's
prints reach the lookup's cover too). Seed bytes only: un-vetoing also lets propagation grow from the seed, so the
rows are a floor on both what an exemption gains and what it lets in. Output outside the repo; refuses judges.

usage: <venv python> -I vetsim.py <index dir> <index id> <out.jsonl> <corpus>=<label> ..."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, HERE)
import lib096  # noqa: E402
from annotate import greedy, SPEC, MIN_NODES, MAX_PKG  # noqa: E402

RIGHT = ("right", "family", "origin")


def collect(corpus, label):
    lib096.use(corpus)
    import score
    import truth_audit
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    run_dir = os.path.join(lib096.DATA, "results", label)
    out = []
    for entry in lib096.load_manifest()["entries"]:
        if entry["truth"]["method"] != "sourcemap":
            continue
        p = os.path.join(run_dir, "reports", entry["id"] + ".json")
        if not os.path.exists(p):
            continue
        doc = json.load(open(p))
        rep = doc["libraryDiscovery"]
        if rep is None or not rep.get("vetoed"):
            continue
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        audit = truth_audit.Audit(entry, truth, cm)
        units = score.run_units(rep)
        scored = {u["i"] for u in score.scored_units(units)}
        lab = score.label_units(truth, units, origins, audit)
        key = {(u["s"], u["e"]): u["i"] for u in units}
        grp = collections.defaultdict(int)
        nunits = collections.Counter()
        for u in units:
            if u.get("m") is not None:
                grp[u["m"]] += u["b"]
                nunits[u["m"]] += 1
        app = truth["appPackage"]
        for v in rep["vetoed"]:
            i = key.get((v["start"], v["end"]))
            if i is None or i not in scored:
                continue
            u = units[i]
            t = lab[i]
            verdict = score.judge_origin(v["package"], t, app)[0]
            m = u.get("m")
            out.append({"set": entry["set"], "entry": entry["id"], "i": i, "bytes": u["b"], "kind": u["k"],
                        "package": v["package"], "truth": t.get("pkg", t["label"]), "verdict": verdict,
                        "right": verdict in RIGHT, "library": "pkg" in t,
                        "modShare": round(u["b"] / grp[m], 3) if m is not None and grp[m] else None,
                        "modUnits": nunits[m] if m is not None else 0, "mb": u["mb"],
                        "slice": doc["toolchain"]["slice"], **({"w": u["w"], "f": u["f"]} if "w" in u else {})})
    return out


def main():
    ixdir, ixid, outp = sys.argv[1:4]
    rows = []
    for arg in sys.argv[4:]:
        corpus, label = arg.split("=", 1)
        if corpus in lib096.PRIVATE:
            raise SystemExit("REFUSED: never diagnose a frozen judge")
        rows += collect(corpus, label)
        print(corpus, len(rows), flush=True)
    import score
    for sl in sorted({r["slice"] for r in rows}):
        ix = score.load_index(ixdir, sl, ixid)
        for r in rows:
            if r["slice"] != sl or "w" not in r:
                continue
            fam = lambda p: score.same_family(p, r["package"])  # noqa: E731
            ok = lambda x: x[3] <= SPEC[0] * r["mb"] + SPEC[1]  # noqa: E731
            nodes = r["w"][1]
            whole = {x[0] for x in ix.units.get(r["w"][0], ()) if ok(x)}
            use_whole = nodes >= MIN_NODES and r["w"][2] >= 3 and whole and len(whole) <= MAX_PKG
            if use_whole:
                r["exact"] = True
                r["ambiguous"] = any(not fam(p) for p in whole)
            else:
                per = collections.defaultdict(list)
                for f in r["f"]:
                    if f[3] < MIN_NODES:
                        continue
                    o = {x[0] for x in ix.units.get(f[2], ()) if ok(x)}
                    if o and len(o) <= MAX_PKG:
                        for p in o:
                            per[p].append(f)
                r["exact"] = False
                r["ambiguous"] = any(not fam(p) and greedy(fs) >= 0.5 * nodes for p, fs in per.items())
        del ix
        print("slice", sl, flush=True)
    with open(outp, "w") as fh:
        for r in rows:
            r.pop("w", None)
            r.pop("f", None)
            fh.write(json.dumps(r) + "\n")


if __name__ == "__main__":
    main()
