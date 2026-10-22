"""exp137 scorer 14, rule `mapdefect` (truth_audit.Audit.mapdefect): the BOTH-DIRECTIONS ledger of labels — every
unit the rule relabels, how each claim's verdict moves (rule off -> on), what it does to recall's truth (unclaimed
units relabelled), and how much of the label's library bytes the layer read (coverage: a unit whose span no
reference run held is left alone).

usage: <venv python> -I ledger14.py --corpus C <label>... [--json out.json]
       <venv python> -I ledger14.py --corpus C <label> --pool <out.jsonl>   (dev corpora only: every relabelled
       unit and every wrong-family claim the rule left alone, with the layer's verdict, for audit14.py)
Prints aggregates per set only (no names). Read from the frozen truth, the layer and the label's reports only."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402

NAMED = ("right", "family", "origin")


def evaluated(entry):
    import gzip
    p = os.path.join(lib096.DATA, "truth", entry["id"] + ".mapdefect14.json.gz")
    with gzip.open(p, "rt") as fh:
        return {(x[0], x[1]): x[2] for x in json.load(fh)["evaluated"]}


def entry_ledger(entry, label, ctx, pool):
    p = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
    if not os.path.exists(p):
        return None
    rep = json.load(open(p))["libraryDiscovery"]
    if rep is None:
        return None
    truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), ctx["chm"])
    origins = truth_audit.origin_layer(entry, ctx["om"], ctx["o7m"])
    truth, origins = truth_audit.redecode(entry, truth, origins)
    units = score.run_units(rep)
    audit = truth_audit.Audit(entry, truth, ctx["cm"])
    on = score.label_units(truth, units, origins, audit)
    layer, audit.md14 = audit.md14, {}
    off = score.label_units(truth, units, origins, audit)
    audit.md14 = layer
    ev = evaluated(entry)
    claims = {(c["start"], c["end"]): c for c in rep.get("claims", [])}
    a, mv = collections.Counter(), collections.Counter()
    app = truth["appPackage"]
    for u in score.scored_units(units):
        t1, t0 = on[u["i"]], off[u["i"]]
        why = ev.get((u["s"], u["e"]), "skipped: copy origin / scorer 13" if "origin" in t0 or "misattributedFrom"
                     in t0 else "span not in the reference runs")
        if t0["label"] not in score.NOT_LIBRARY:
            a["library bytes"] += u["b"]
            if (u["s"], u["e"]) in ev:
                a["library bytes the layer read"] += u["b"]
        c = claims.get((u["s"], u["e"]))
        v0 = score.judge_origin(c["package"], t0, app)[0] if c else None
        v1 = score.judge_origin(c["package"], t1, app)[0] if c else None
        if "mapDefectFrom" in t1:
            a["relabelled units"] += 1
            a["relabelled bytes"] += u["b"]
            a["relabelled bytes, claimed" if c else "relabelled bytes, unclaimed (recall truth)"] += u["b"]
            if c:
                mv[f"{v0} -> {v1}"] += u["b"]
            if pool is not None:
                pool.append({"kind": "relabel", "corpus": lib096.CORPUS, "set": entry["set"], "entry": entry["id"], "s": u["s"],
                             "e": u["e"], "bytes": u["b"], "unitKind": u["k"], "old": t0["label"],
                             "oldFile": t0.get("file"), "new": t1["label"], "newFile": t1.get("file"),
                             "newVersion": t1.get("version"), "claim": c["package"] if c else None,
                             "v0": v0, "v1": v1})
        elif c and v1 == "wrongFamily" and pool is not None:
            pool.append({"kind": "missed", "corpus": lib096.CORPUS, "set": entry["set"], "entry": entry["id"], "s": u["s"], "e": u["e"],
                         "bytes": u["b"], "unitKind": u["k"], "old": t0["label"], "oldFile": t0.get("file"),
                         "claim": c["package"], "layer": why})
        if c and v1 == "wrongFamily":
            a["wrong-family bytes left, by layer verdict: " + why] += u["b"]
    return a, mv


def main():
    argv = lib096.corpus_flag(sys.argv)
    labels = [x for i, x in enumerate(argv[1:], 1) if not x.startswith("--") and argv[i - 1] not in ("--json", "--pool")]
    pool_path = argv[argv.index("--pool") + 1] if "--pool" in argv else None
    if pool_path and lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: a frozen judge is never audited")
    ctx = {"om": lib096.load_origins_manifest(), "o7m": truth_audit.load_origins7_manifest(),
           "chm": truth_audit.load_chained_manifest(), "cm": truth_audit.load_copies_manifest()}
    out = {}
    pool = [] if pool_path else None
    for label in labels:
        agg = collections.defaultdict(collections.Counter)
        moves = collections.defaultdict(collections.Counter)
        for entry in lib096.load_manifest()["entries"]:
            if entry["truth"]["method"] != "sourcemap":
                continue
            r = entry_ledger(entry, label, ctx, pool)
            if r is None:
                continue
            agg[entry["set"]].update(r[0])
            moves[entry["set"]].update(r[1])
        out[label] = {s: {"counts": dict(sorted(agg[s].items())), "moves": dict(sorted(moves[s].items()))}
                      for s in sorted(agg)}
        print("==", label)
        for s in sorted(agg):
            print(" ", s, dict(sorted(agg[s].items())))
            for k, v in moves[s].most_common():
                print(f"    {v:>12,}  {k}")
    if "--json" in argv:
        open(argv[argv.index("--json") + 1], "w").write(lib096.dump(out))
    if pool_path:
        with open(pool_path, "w") as fh:
            for r in pool:
                fh.write(json.dumps(r, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
