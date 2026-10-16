"""exp120: the scorer-9 relabel LEDGER: what scorer 9 changed in the TRUTH JUDGEMENT, per label, in BOTH directions.

usage: <venv python> -I ledger9.py <scorer 8 families-derived.json> <label>... [--corpus C] --json out.json
       [--rows out.jsonl]
Scorer 9 differs from scorer 8 in two places: the links file family() reads (re-derived over a universe that adds
every truth package name), and rule `npmchain` (truth_audit RULES9: a chained source naming a package npm does not
have keeps the real package whose map listed it). For every claimed statement unit of every entry the label ran,
the unit is labelled exactly as score.py labels it TWICE, with RULES9 off (scorer 8's truth) and on, and the claim
is judged with scorer 8's links on the first labels and scorer 9's on the second. A changed verdict is charged to
`npmchain` when the unit's label (or origin) changed, else to the NEW link kinds that alone join the claimed
package and the judged label (exp119 ledger's Kinds: repo / rename / fork, `chain` for a mix). Classes as exp119:
GOOD (right, family, origin), WRONG (wrongFamily, app), UNJUDGED (the rest). Claude Code's sample truth has no
source table, so only the links can move it (exp119 ledger's sample_entry). On a private corpus (test, test2)
pass --json only: per-set aggregates."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../119-families-full"))
import families_lib  # noqa: E402
import ledger as L119  # noqa: E402  (Kinds, cls, judged, sample_entry)
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402


def prepare(entry, ctx, npmchain):
    om, o7m, cm, chm = ctx
    truth_audit.RULES9["npmchain"] = npmchain
    try:
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
    finally:
        truth_audit.RULES9["npmchain"] = True
    origins = truth_audit.origin_layer(entry, om, o7m)
    truth, origins = truth_audit.redecode(entry, truth, origins)
    return truth, origins, truth_audit.Audit(entry, truth, cm)


def labelled(prep, units):
    truth, origins, audit = prep
    return score.label_units(truth, units, origins, audit)


def map_entry(preps, rep, old, new, kinds, emit):
    (po, pn) = preps
    units = score.run_units(rep)
    st = score.statement_units(units)
    lo, ln = labelled(po, units), labelled(pn, units)
    key = {(u["s"], u["e"]): u for u in st}
    app = po[0]["appPackage"]
    for c in rep.get("claims", []):
        u = key.get((c["start"], c["end"]))
        if u is None:
            continue
        ro, rn = lo[u["i"]], ln[u["i"]]
        vo = L119.judged(old, lambda: score.judge_origin(c["package"], ro, app)[0])
        vn = L119.judged(new, lambda: score.judge_origin(c["package"], rn, app)[0])
        if vo == vn:
            continue
        if (ro["label"], ro.get("origin")) != (rn["label"], rn.get("origin")):
            kind = "npmchain"
        else:
            against = rn.get("origin") if vn in ("origin", "selfOrigin") or vo in ("origin", "selfOrigin") \
                else rn["label"]
            kind = kinds.of(c["package"], against)
        emit(u["b"], vo, vn, kind, {"s": u["s"], "e": u["e"], "claim": c["package"], "oldLabel": ro["label"],
                                    "label": rn["label"], "origin": rn.get("origin")})


def main():
    argv = lib096.corpus_flag(sys.argv)
    old_path = argv[1]
    labels = [a for a in argv[2:] if not a.startswith("--") and not a.endswith((".json", ".jsonl"))]
    jout = argv[argv.index("--json") + 1]
    rout = argv[argv.index("--rows") + 1] if "--rows" in argv else None
    if rout and lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: rows name private-corpus apps; aggregates only")
    old, new = families_lib.Families(old_path), families_lib.Families()
    kinds = L119.Kinds(families_lib.DERIVED)
    ctx = (lib096.load_origins_manifest(), truth_audit.load_origins7_manifest(),
           truth_audit.load_copies_manifest(), truth_audit.load_chained_manifest())
    pipes = {label: json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
             for label in labels}
    trans = {label: collections.Counter() for label in labels}
    rows = []
    for entry in lib096.load_manifest()["entries"]:
        preps = None
        for label in labels:
            if pipes[label].get("sets") and entry["set"] not in pipes[label]["sets"]:
                continue
            rp = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
            rep = json.load(open(rp))["libraryDiscovery"]
            if rep is None:
                continue

            def emit(b, vo, vn, kind, extra, entry=entry, label=label):
                trans[label][(entry["set"], kind, vo, vn)] += b
                if rout:
                    rows.append({"label": label, "entry": entry["id"], "set": entry["set"], "b": b,
                                 "kind": kind, "old": vo, "new": vn, **extra})
            if entry["truth"]["method"] == "sample":
                L119.sample_entry(entry, rep, old, new, kinds, emit)
            elif entry["truth"]["method"] == "sourcemap":
                if preps is None:
                    preps = (prepare(entry, ctx, False), prepare(entry, ctx, True))
                map_entry(preps, rep, old, new, kinds, emit)
    result = {}
    for label in labels:
        per_set = collections.defaultdict(lambda: collections.Counter())
        for (s, kind, vo, vn), b in trans[label].items():
            per_set[s][f"{L119.cls(vo)}->{L119.cls(vn)}"] += b
            per_set[s][f"{L119.cls(vo)}->{L119.cls(vn)}:{kind}"] += b
        result[label] = {"corpus": lib096.CORPUS,
                         "transitions": [[*k, round(v, 1)] for k, v in sorted(trans[label].items())],
                         "perSet": {s: {k: round(v, 1) for k, v in sorted(c.items())}
                                    for s, c in sorted(per_set.items())}}
        print(label, json.dumps(result[label]["perSet"]), flush=True)
    open(jout, "w").write(lib096.dump(result))
    if rout:
        with open(rout, "w") as fh:
            for r in rows:
                fh.write(json.dumps(r, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
