"""exp104: the relabel LEDGER — what scorer 4's truth changed, per rule, in BOTH directions.

usage: <venv python> -I ledger.py <label>... [--json out.json] [--rows out.jsonl] [--corpus dev|test]
(on the test corpus only --json, per-set aggregates, is for reading: the rows name test apps)
For every statement unit of every source-mapped entry, the scorer-3 truth (no audit, families.json families) and
the scorer-4 truth (truth_audit + families_lib) label it; a claimed unit is judged by each. Each changed unit is
charged to ONE rule, in order: `carried` (the new label is <unmapped> through the carried rule), `copy` (the new
origin came from the copies layer), `family` (same label, the judgement changed through a derived family link;
sub-rule repo / rename / fork = the link kinds that alone join the pair). Verdict classes: GOOD (right, family,
origin), WRONG (wrongFamily, app), UNJUDGED (self, selfOther, selfOrigin, unmapped, foreign), NONE (unclaimed).
Library bytes (the recall denominator) are counted under both truths. Claude Code (sample truth) has no map: the
family change is the only one that can reach it, reported apart from the cards' estimate (sample units)."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.join(HERE, "../096-library-matching-score")
sys.path.insert(0, SCORE)
import families_lib  # noqa: E402
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402
from libix import family as base_family  # noqa: E402

GOOD = ("right", "family", "origin")
WRONG = ("wrongFamily", "app")


def cls(v):
    if v is None:
        return "NONE"
    return "GOOD" if v in GOOD else "WRONG" if v in WRONG else "UNJUDGED"


def old_judge(claimed, row, app):
    """scorer 3's judge_origin, verbatim but for the family table (families.json only)."""
    lab = row["label"]
    if lab == "<self>":
        old = "self" if claimed == app else "selfOther"
    elif lab == "<app>":
        old = "app"
    elif lab in ("<unmapped>", "<foreign>"):
        old = lab.strip("<>")
    elif claimed == lab:
        old = "right"
    else:
        old = "family" if base_family(claimed) == base_family(lab) else "wrongFamily"
    o = row.get("origin")
    if o is not None and old in ("wrongFamily", "selfOther") and base_family(claimed) == base_family(o):
        return "origin" if old == "wrongFamily" else "selfOrigin"
    return old


class SubRules:
    """Which derived link kinds alone join a pair."""

    def __init__(self):
        links = json.load(open(families_lib.DERIVED))["links"]
        self.f = {}
        for kind in ("repo", "rename", "fork"):
            path = os.path.join(os.environ.get("TMPDIR", "/tmp"), f"exp104-links-{kind}-{os.getpid()}.json")
            json.dump({"links": [x for x in links if x["rule"] == kind]}, open(path, "w"))
            self.f[kind] = families_lib.Families(path)
            os.unlink(path)

    def of(self, a, b):
        return "+".join(k for k, f in self.f.items() if f.linked(a, b)) or "chain"


def main():
    argv = lib096.corpus_flag(sys.argv)
    labels = [a for a in argv[1:] if not a.startswith("--") and not a.endswith((".json", ".jsonl"))]
    jout = argv[argv.index("--json") + 1] if "--json" in argv else None
    rout = argv[argv.index("--rows") + 1] if "--rows" in argv else None
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    sub = SubRules()
    result, rows = {}, []
    entries = [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    cache = {}
    for label in labels:
        trans = collections.Counter()  # (set, rule, old class, new class) -> bytes
        denom = collections.Counter()  # (set, "old"/"new") -> library bytes
        for entry in entries:
            doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")))
            rep = doc["libraryDiscovery"]
            if rep is None:
                continue
            if entry["id"] not in cache:
                truth = lib096.load_truth(entry)
                cache.clear()
                cache[entry["id"]] = (truth, lib096.load_origins(entry, om), truth_audit.Audit(entry, truth, cm))
            truth, org, audit = cache[entry["id"]]
            units = score.run_units(rep)
            st = score.statement_units(units)
            old = score.label_units(truth, units, org)
            new = score.label_units(truth, units, org, audit)
            key = {(u["s"], u["e"]): u["i"] for u in st}
            claims = {key[(c["start"], c["end"])]: c["package"] for c in rep.get("claims", [])
                      if (c["start"], c["end"]) in key}
            app = truth["appPackage"]
            for u in st:
                i, b = u["i"], u["b"]
                lo, ln = old[i], new[i]
                denom[(entry["set"], "old")] += b if lo["label"] not in score.NOT_LIBRARY else 0
                denom[(entry["set"], "new")] += b if ln["label"] not in score.NOT_LIBRARY else 0
                c = claims.get(i)
                vo = old_judge(c, lo, app) if c else None
                vn = score.judge_origin(c, ln, app)[0] if c else None
                if ln.get("carriedFrom") is not None:
                    rule = "carried"
                elif ln.get("originEvidence") == "copy" and (vo != vn or c is None):
                    rule = "copy"
                elif vo != vn:
                    rule = "family:" + sub.of(c, ln.get("origin") if vn == "origin" else ln["label"])
                else:
                    continue
                trans[(entry["set"], rule, cls(vo), cls(vn))] += b
                rows.append({"label": label, "entry": entry["id"], "set": entry["set"], "s": u["s"], "e": u["e"],
                             "b": b, "rule": rule, "oldLabel": lo["label"], "newLabel": ln["label"],
                             "origin": ln.get("origin"), "claim": c, "old": vo, "new": vn})
        result[label] = {"transitions": [[*k, v] for k, v in sorted(trans.items())],
                         "libraryBytes": {f"{s}:{w}": v for (s, w), v in sorted(denom.items())}}
        print(label, json.dumps(result[label]["libraryBytes"]), flush=True)
    if jout:
        open(jout, "w").write(lib096.dump(result))
    if rout:
        with open(rout, "w") as fh:
            for r in rows:
                fh.write(json.dumps(r, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
