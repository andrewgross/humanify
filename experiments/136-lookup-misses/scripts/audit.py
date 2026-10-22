"""exp136: how much of two labels' wrong-family bytes is the TRUTH's source-map defect? For every claim a judge13.py
file marks wrongFamily (slot c0 and c1), read the unit's name evidence (dcheck.unit_evidence: acorn tokens) and ask
whether the truth's package and the claimer's package hold it in their installed files (the union of each package's
files over its newest installs in the index's work dir, .js/.cjs/.mjs/.json):
  map-defect     the truth's package lacks the unit's code and the claimer's package holds it: the bundle's map
                 credited another package's file with the claimer's code (Bun 1.4; exp132/exp135/exp136), so the
                 claim is right and the scorer calls it wrong
  claimer-lacks  the claimer's package does not hold the unit's code (a real wrong claim)
  both-hold      both packages hold it (a copy / carrier: scorer 13's verdict stands)
  truth-only ... and no-evidence (under three tokens), unknown (a package not installed)
Prints per set and label: wrong KB, the map-defect KB, and precision with the map-defect bytes counted as named
(an AUDIT column: the scorer is unchanged). Aggregates only; no package names.

usage: python3 -I audit.py <corpus> <judge.jsonl> <label0> <label1> [--json out]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import dcheck  # noqa: E402
import lib096  # noqa: E402
import whohas  # noqa: E402

RESULTS = os.path.join(HERE, "../../096-library-matching-score/results")


def holds(pkg, toks, cache):
    k = (pkg, tuple(sorted(toks)))
    if k not in cache:
        dirs = whohas.claimer_dirs(pkg)
        cache[k] = None if not dirs else any(
            sum(any(t in text for q, _, text in whohas.files_of(wd) if q == pkg) for t in toks) >= 0.5 * len(toks)
            for wd in dirs)
    return cache[k]


def main():
    corpus, path, l0, l1 = sys.argv[1:5]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    agg = collections.Counter()
    cache = {}
    byentry = collections.defaultdict(list)
    for line in open(path):
        r = json.loads(line)
        if any((r.get(s) or {}).get("verdict") == "wrongFamily" for s in ("c0", "c1")):
            byentry[r["entry"]].append(r)
    for eid, rs in byentry.items():
        toks_all = dcheck.acorn_evidence(dcheck.bundle_path(eid), [[r["start"], r["end"]] for r in rs])
        for r, toks in zip(rs, toks_all):
            for slot in ("c0", "c1"):
                c = r.get(slot)
                if not c or c["verdict"] != "wrongFamily":
                    continue
                if len(toks) < 3:
                    cls = "no-evidence"
                else:
                    t = holds(r["truth"], toks, cache) if r.get("truth") and not r["truth"].startswith("<") else None
                    w = holds(c["package"], toks, cache)
                    cls = ("unknown" if t is None or w is None else "map-defect" if (not t and w) else
                           "claimer-lacks" if not w else "both-hold")
                agg[(r["set"], slot, cls)] += r["bytes"]
    out = []
    for slot, label in (("c0", l0), ("c1", l1)):
        sm = json.load(open(os.path.join(RESULTS, label, "summary.json")))["sets"]
        for s, v in sorted(sm.items()):
            if not any(k[0] == s and k[1] == slot for k in agg):
                continue
            md = agg[(s, slot, "map-defect")]
            n, w = v["namedBytes"], v["wrongFamilyBytes"]
            rec = {"set": s, "label": label, "wrongKB": round(w / 1e3, 1), "mapDefectKB": round(md / 1e3, 1),
                   **{k[2]: round(b / 1e3, 1) for k, b in agg.items() if k[0] == s and k[1] == slot},
                   "precision": round(100 * n / (n + w), 3), "auditedPrecision": round(100 * (n + md) / (n + w), 3)}
            out.append(rec)
            print(json.dumps(rec))
    if "--json" in sys.argv:
        json.dump(out, open(sys.argv[sys.argv.index("--json") + 1], "w"), indent=1)


if __name__ == "__main__":
    main()
