"""exp119: the scorer-8 relabel LEDGER: what re-deriving families-derived.json over every package of the default
index (k2-all) changed in the TRUTH JUDGEMENT, per label, in BOTH directions.

usage: <venv python> -I ledger.py <old families-derived.json> <label>... [--corpus C] --json out.json
       [--rows out.jsonl]
Scorer 8 differs from scorer 7 ONLY in the links file family() reads. For every claimed statement unit of every
entry the label ran, the unit is labelled exactly as score.py labels it (chained sources, origin layers, decoder
rules, the exp104 audit), then the claim is judged twice: with the OLD links (scorer 7's file, passed in) and the
NEW ones (the scorer's own file). A changed verdict is charged to the link kinds of the NEW file that alone join
the claimed package and the judged label (repo / rename / fork, `chain` when only a mix of kinds does). Classes:
GOOD (right, family, origin), WRONG (wrongFamily, app), UNJUDGED (self, selfOther, selfOrigin, unmapped, foreign).
The recall denominator does not depend on family(), so it cannot move; misses can only move with the claims.
Claude Code's sample truth (no map) is judged by span containment as score_sample does, in ESTIMATED bytes.
On a private corpus (test, test2) pass --json only: per-set aggregates; --rows is refused there."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.join(HERE, "../096-library-matching-score")
sys.path.insert(0, SCORE)
import bisect  # noqa: E402

import families_lib  # noqa: E402
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402

GOOD = ("right", "family", "origin")
WRONG = ("wrongFamily", "app")


def cls(v):
    return "GOOD" if v in GOOD else "WRONG" if v in WRONG else "UNJUDGED"


class Kinds:
    """Which NEW link kinds alone join a pair."""

    def __init__(self, path):
        links = json.load(open(path))["links"]
        self.f = {}
        for kind in ("repo", "rename", "fork"):
            tmp = os.path.join(os.environ.get("TMPDIR", "/tmp"), f"exp119-links-{kind}-{os.getpid()}.json")
            json.dump({"links": [x for x in links if x["rule"] == kind]}, open(tmp, "w"))
            self.f[kind] = families_lib.Families(tmp)
            os.unlink(tmp)

    def of(self, a, b):
        return "+".join(k for k, f in self.f.items() if f.linked(a, b)) or "chain"


def judged(fams, fn):
    score.same_family = fams.same
    return fn()


def prepare(entry, ctx):
    """The entry's truth exactly as score.main builds it (chained sources, origin layers, decoder rules)."""
    om, o7m, cm, chm = ctx
    truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
    origins = truth_audit.origin_layer(entry, om, o7m)
    truth, origins = truth_audit.redecode(entry, truth, origins)
    return entry, truth, origins, truth_audit.Audit(entry, truth, cm)


def map_entry(prep, rep, old, new, kinds, emit):
    entry, truth, origins, audit = prep
    units = score.run_units(rep)
    st = score.statement_units(units)
    labels = score.label_units(truth, units, origins, audit)
    key = {(u["s"], u["e"]): u for u in st}
    app = truth["appPackage"]
    for c in rep.get("claims", []):
        u = key.get((c["start"], c["end"]))
        if u is None:
            continue
        row = labels[u["i"]]
        vo = judged(old, lambda: score.judge_origin(c["package"], row, app)[0])
        vn = judged(new, lambda: score.judge_origin(c["package"], row, app)[0])
        if vo != vn:
            against = row.get("origin") if vn in ("origin", "selfOrigin") or vo in ("origin", "selfOrigin") \
                else row["label"]
            emit(u["b"], vo, vn, kinds.of(c["package"], against),
                 {"s": u["s"], "e": u["e"], "claim": c["package"], "label": row["label"],
                  "origin": row.get("origin")})


def sample_entry(entry, rep, old, new, kinds, emit):
    truth = lib096.load_truth(entry)
    strata = truth["strata"]
    sb = collections.Counter()
    for t in truth["rows"]:
        sb[t["stratum"]] += t["b"]
    cl = sorted((c["start"], c["end"], c["package"]) for c in rep.get("claims", []))
    starts = [c[0] for c in cl]
    for t in truth["rows"]:
        k = bisect.bisect_right(starts, t["s"]) - 1
        p = cl[k][2] if k >= 0 and cl[k][0] <= t["s"] < cl[k][1] else None
        if p is None or "pkg" not in t or p == t["pkg"]:
            continue
        vo = "family" if old.same(p, t["pkg"]) else "wrongFamily"
        vn = "family" if new.same(p, t["pkg"]) else "wrongFamily"
        if vo != vn:
            emit(strata[t["stratum"]]["bytes"] * t["b"] / sb[t["stratum"]], vo, vn, kinds.of(p, t["pkg"]),
                 {"s": t["s"], "claim": p, "label": t["pkg"], "sampleBytes": t["b"]})


def main():
    argv = lib096.corpus_flag(sys.argv)
    old_path = argv[1]
    labels = [a for a in argv[2:] if not a.startswith("--") and not a.endswith((".json", ".jsonl"))]
    jout = argv[argv.index("--json") + 1]
    rout = argv[argv.index("--rows") + 1] if "--rows" in argv else None
    if rout and lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: rows name private-corpus apps; aggregates only")
    old, new = families_lib.Families(old_path), families_lib.Families()
    kinds = Kinds(families_lib.DERIVED)
    ctx = (lib096.load_origins_manifest(), truth_audit.load_origins7_manifest(),
           truth_audit.load_copies_manifest(), truth_audit.load_chained_manifest())
    pipes = {label: json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
             for label in labels}
    trans = {label: collections.Counter() for label in labels}
    rows = []
    # entries outer, labels inner: one entry's truth is decoded once for every label (the costly part)
    for entry in lib096.load_manifest()["entries"]:
        prep = None
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
                sample_entry(entry, rep, old, new, kinds, emit)
            elif entry["truth"]["method"] == "sourcemap":
                if prep is None:
                    prep = prepare(entry, ctx)
                map_entry(prep, rep, old, new, kinds, emit)
    result = {}
    for label in labels:
        per_set = collections.defaultdict(lambda: collections.Counter())
        for (s, kind, vo, vn), b in trans[label].items():
            per_set[s][f"{cls(vo)}->{cls(vn)}"] += b
            per_set[s][f"{cls(vo)}->{cls(vn)}:{kind}"] += b
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
