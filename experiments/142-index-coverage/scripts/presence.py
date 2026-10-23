"""exp142: could indexing the missing version recover a missed unit? For every MISSED unit (misses.py rows) of the
classes a delta index would touch (after-asof, not-chosen) and, as the control, `held`, ask the index slice the
bundle reads whether the truth package's FAMILY already holds the unit's prints at any version:
  whole-held   a row of the family carries the unit's whole print: the miss is not the version (the lookup or a
               guard refused it); a new version adds a second copy of a print the family already holds
  fn-held      no whole print, but >= 1 of its function prints (>= 30 nodes) is the family's
  unheld       the family holds none of its prints: only a new print (a new version's) could name it
  no-prints    the unit has no print in the run (a whole factory kept as one unit, unit-model)
An upper bound on what a delta of these versions could add is the `unheld` bytes (+ part of fn-held); the
control's `unheld` share says how often a HELD version still leaves a unit unheld (drop rules, print shape).

usage: <venv python> -I presence.py <out.json> <corpus>=<misses.jsonl> ...   (index = the label's, 9d40265a)"""
import collections
import datetime
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, HERE)
import lib096  # noqa: E402
import score  # noqa: E402
import classify  # noqa: E402

IX = "/work/exp135-data/full/ix/k2-all-r5-origin-prod"
IX_ID = "9d40265aa1eed02c"
CLASSES = ("after-asof", "not-chosen", "held")


def presence(ix, r):
    if "w" not in r:
        return "no-prints"
    fam = lambda o: score.same_family(o, r["pkg"])  # noqa: E731
    if any(fam(x[0]) for x in ix.units.get(r["w"][0], ())):
        return "whole-held"
    if any(n >= 30 and any(fam(x[0]) for x in ix.units.get(h, ())) for h, n in r["f"]):
        return "fn-held"
    return "unheld"


def main():
    out = sys.argv[1]
    versions = {k: set(v) for k, v in json.load(open(os.path.join(IX, "versions.json"))).items()}
    reg = classify.Registry(["/work/exp142-data/registry-fresh", "/work/exp130-data/full/registry"])
    rows = collections.defaultdict(list)  # slice -> rows
    for arg in sys.argv[2:]:
        corpus, path = arg.split("=", 1)
        for line in open(path):
            r = json.loads(line)
            if r["named"]:
                continue
            c, _ = classify.classify(r, versions, reg)
            if c in CLASSES:
                r["class"] = c
                rows[r["slice"]].append(r)
    res = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    for sl, rs in sorted(rows.items()):
        ix = score.load_index(IX, sl, IX_ID)
        for r in rs:
            res[r["set"]][r["class"]][presence(ix, r)] += r["bytes"]
        del ix
    doc = {s: {c: {k: round(b / 1024, 1) for k, b in sorted(cnt.items())} for c, cnt in sorted(d.items())}
           for s, d in sorted(res.items())}
    json.dump(doc, open(out, "w"), indent=1, sort_keys=True)
    for s, d in doc.items():
        print(f"{s:16s} " + " | ".join(f"{c}: " + " ".join(f"{k}={v:.0f}" for k, v in cnt.items())
                                       for c, cnt in d.items()), flush=True)


if __name__ == "__main__":
    main()
