"""exp143: every library unit scorer 14 counts as MISSED (unclaimed, or claimed as the wrong family / app) whose
truth (package, version) the index HOLDS (versions.json), with what the census needs beside exp142's misses.py row:
the unit's offsets, kind, module group, prints (whole + functions with offsets, nodes, content), the share of its
bytes the truth's map credits to the labelled package, and what the run's own report says about it and its
neighbours (claim rule, vetoed seed, library-class closure, queue head; the truth family's claims in the same
module group and within the lookup's support window). Output stays OUTSIDE the repo (it names packages). Refuses
the frozen judges.

usage: <venv python> -I rows.py <corpus> <label> <index dir> <out.jsonl>"""
import bisect
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402

SUPPORT = 10  # LookupRules::support


def mapped_share(truth, offs, s, e, lab):
    segs, srcs = truth["segments"], truth["sources"]
    k = max(bisect.bisect_right(offs, s) - 1, 0)
    votes = collections.Counter()
    while k < len(segs) and segs[k][0] < e:
        a, b = max(segs[k][0], s), min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
        if b > a:
            votes[srcs[segs[k][1]][0]] += b - a
        k += 1
    return round(votes[lab] / max(e - s, 1), 3), round(sum(votes.values()) / max(e - s, 1), 3)


def main():
    corpus, label, ixdir, out = sys.argv[1:5]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never diagnose a frozen judge")
    lib096.use(corpus)
    import score
    import truth_audit
    versions = {k: set(v) for k, v in json.load(open(os.path.join(ixdir, "versions.json"))).items()}
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    run_dir = os.path.join(lib096.DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    fh = open(out, "w")
    tot = collections.Counter()
    for entry in lib096.load_manifest()["entries"]:
        if entry["truth"]["method"] != "sourcemap":
            continue
        if pipeline.get("sets") and entry["set"] not in pipeline["sets"]:
            continue
        doc = json.load(open(os.path.join(run_dir, "reports", entry["id"] + ".json")))
        rep, slice_ = doc["libraryDiscovery"], doc["toolchain"]["slice"]
        if rep is None:
            continue
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        audit = truth_audit.Audit(entry, truth, cm)
        units = score.run_units(rep)
        scope = {u["i"]: "statement" for u in score.statement_units(units)}
        scope.update({u["i"]: "factory" for u in score.factory_units(units)})
        every = score.scored_units(units)
        lab = score.label_units(truth, units, origins, audit)
        key = {(u["s"], u["e"]): u["i"] for u in units}
        claims = {key[(c["start"], c["end"])]: c for c in rep.get("claims", []) if (c["start"], c["end"]) in key}
        vetoed = {key[(c["start"], c["end"])]: c["package"] for c in rep.get("vetoed", [])
                  if (c["start"], c["end"]) in key}
        lclass = {key[(c["start"], c["end"])]: c["package"] for c in rep.get("libraryClass", [])
                  if (c["start"], c["end"]) in key}
        queued = {key[(c["start"], c["end"])] for c in rep.get("queue", []) if (c["start"], c["end"]) in key}
        app = truth["appPackage"]
        offs = [x[0] for x in truth["segments"]]
        modcache = {}
        # the lookup's query order: statement units with prints (statements, lazy bodies, factory bodies)
        q = [u["i"] for u in every if "w" in u]
        qpos = {i: n for n, i in enumerate(q)}
        bymod = collections.defaultdict(list)
        for u in every:
            if u.get("m") is not None:
                bymod[u["m"]].append(u["i"])
        for u in every:
            t = lab[u["i"]]
            if "pkg" not in t:
                continue
            c = claims.get(u["i"])
            v = score.judge_origin(c["package"], t, app)[0] if c else "unclaimed"
            named = v in ("right", "family", "origin")
            tot[(entry["set"], "library")] += u["b"]
            if named:
                continue
            held = t.get("version") in versions.get(t["pkg"], ())
            tot[(entry["set"], "held" if held else "notheld")] += u["b"]
            if not held:
                continue
            fam = lambda p: p is not None and (score.same_family(p, t["pkg"]) or any(  # noqa: E731
                o is not None and score.same_family(p, o) for o in (t.get("origin"), t.get("via"))))
            mod = bymod.get(u.get("m"), [])
            mk = (u.get("m"), t["pkg"], t.get("origin"), t.get("via"))
            if mk not in modcache:
                modcache[mk] = (sum(units[j]["b"] for j in mod),
                                sum(units[j]["b"] for j in mod if j in claims and fam(claims[j]["package"])),
                                sum(units[j]["b"] for j in mod if j in claims and not fam(claims[j]["package"])))
            mod_b, mod_fam, mod_other = modcache[mk]
            win = []
            if u["i"] in qpos:
                n = qpos[u["i"]]
                win = [q[j] for j in range(max(0, n - SUPPORT), min(len(q), n + SUPPORT + 1)) if j != n]
            share, mapped = mapped_share(truth, offs, u["s"], u["e"], t["label"])
            row = {"entry": entry["id"], "set": entry["set"], "i": u["i"], "s": u["s"], "e": u["e"],
                   "bytes": u["b"], "scope": scope[u["i"]], "kind": u["k"], "m": u.get("m"), "mb": u["mb"],
                   "pkg": t["pkg"], "version": t.get("version"), "file": t.get("file"), "origin": t.get("origin"),
                   "via": t.get("via"), "claimed": c["package"] if c else None, "rule": c["rule"] if c else None,
                   "verdict": v, "slice": slice_, "share": share, "mapped": mapped,
                   "vetoed": vetoed.get(u["i"]), "lclass": lclass.get(u["i"]), "queued": u["i"] in queued,
                   "modUnits": len(mod), "modBytes": mod_b, "modFam": mod_fam, "modOther": mod_other,
                   "winFam": sum(1 for j in win if j in claims and fam(claims[j]["package"])),
                   "winFamSeed": sum(1 for j in win if j in claims and claims[j]["rule"] == "seed"
                                     and fam(claims[j]["package"])),
                   "mapDefectFrom": t.get("mapDefectFrom"), "misattributedFrom": t.get("misattributedFrom")}
            if "w" in u:
                row["w"] = u["w"]
                row["f"] = u["f"]
            fh.write(json.dumps(row) + "\n")
    json.dump({f"{s}|{k}": b for (s, k), b in sorted(tot.items())}, open(out + ".totals.json", "w"), indent=1)


if __name__ == "__main__":
    main()
