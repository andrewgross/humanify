"""exp102: decompose a label's wrong-family claims by MECHANISM (not by package).

usage: <venv python> -I wrongfam.py <label> [--rows out.jsonl]
reads the same reports, truth and origins as exp096's score.py (its functions, imported), and for every claimed
statement unit the scorer judges wrongFamily prints one row: set, entry, claimed package, truth package/origin,
bytes, the claim's rule, and a cause class:

  seed:*      the index seed itself was wrong. Sub-class from re-running the structural rule on the unit:
              seed:whole-shared   the whole print hit, and more than one family holds it (fork / shared code)
              seed:whole-unique   the whole print hit, held by the claimed family alone (vendored copy, generated)
              seed:fn-shared      a function cover, more than one family accepted
              seed:fn-unique      a function cover, one family
              seed:small-nested   a function cover whose largest covering function is < 1/4 of the unit's nodes
              seed:declared / seed:banner / seed:queue-index (non-index evidence or the targeted round)
  prop:*      a propagated claim (down / module / glue), split by where its package's seeds stand in the bundle:
              prop:from-wrong-seeds   every seed of the claimed package in this bundle is itself wrong-family
              prop:overreach          the claimed package has right seeds here: propagation ran past its region
Only the summary is printed (cause class -> bytes per set); --rows writes every row for reading.
"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.join(HERE, "../096-library-matching-score")
sys.path.insert(0, SCORE)
import lib096  # noqa: E402
import mmlib  # noqa: E402
import score  # noqa: E402
from lib096 import DATA  # noqa: E402


def families_accepting(ix, u):
    """(families the structural rule accepts on its own, whole hit?, largest covering print's nodes)."""
    whole, ok_w, ok_f = mmlib.eligible(u)
    spec = mmlib.RULES["spec"]
    floor = mmlib.RULES["cover"] * max(whole[3], 1)
    if ok_w:
        o = ix.owners(whole[2], u["mb"], spec)
        if o and len(o) <= mmlib.RULES["max_packages"]:
            return {mmlib.family(p) for p in o}, True, whole[3]
    score_ = collections.Counter()
    biggest = collections.Counter()
    hits = []
    for f in ok_f:
        o = ix.owners(f[2], u["mb"], spec)
        if o and len(o) <= mmlib.RULES["max_packages"]:
            hits.append((f, o))
    hits.sort(key=lambda x: -x[0][3])
    taken = []
    for f, o in hits:
        if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        for p in o:
            fam = mmlib.family(p)
            score_[fam] += f[3]
            biggest[fam] = max(biggest[fam], f[3])
    acc = {p for p, v in score_.items() if v >= floor}
    return acc, False, max(biggest.values(), default=0)


def seed_class(ix, u, claimed):
    acc, whole, big = families_accepting(ix, u)
    shared = len(acc) > 1
    if whole:
        return "seed:whole-shared" if shared else "seed:whole-unique"
    if big * 4 < u["w"][1]:
        return "seed:small-nested"
    return "seed:fn-shared" if shared else "seed:fn-unique"


def rows_for(entry, doc, truth, origins, ix):
    report = doc["libraryDiscovery"]
    units = score.run_units(report)
    st = score.statement_units(units)
    by_i = {u["i"]: u for u in units}
    labels = score.label_units(truth, units, origins)
    app_pkg = truth["appPackage"]
    key = {(u["s"], u["e"]): u["i"] for u in st}
    claims = {key[(c["start"], c["end"])]: c for c in report.get("claims", []) if (c["start"], c["end"]) in key}
    verdict = {i: score.judge_origin(c["package"], labels[i], app_pkg)[0] for i, c in claims.items()}
    seed_ok = collections.defaultdict(lambda: [0, 0])  # package -> [right seeds, wrong seeds]
    for i, c in claims.items():
        if c["rule"] == "seed":
            seed_ok[c["package"]][0 if verdict[i] in ("right", "family", "origin") else 1] += 1
    out = []
    for i, c in claims.items():
        if verdict[i] != "wrongFamily":
            continue
        u = by_i[i]
        ev = (c.get("evidence") or {}).get("kind")
        if c["rule"] == "seed":
            cls = seed_class(ix, u, c["package"]) if ev == "index" and "w" in u else f"seed:{ev}"
        else:
            cls = "prop:overreach" if seed_ok[c["package"]][0] else "prop:from-wrong-seeds"
        lab = labels[i]
        out.append({"entry": entry["id"], "set": entry["set"], "start": u["s"], "end": u["e"], "bytes": u["b"],
                    "claimed": c["package"], "truth": lab.get("label"), "origin": lab.get("origin"),
                    "rule": c["rule"], "round": c["round"], "cause": cls})
    return out


def main():
    label = sys.argv[1]
    rows_out = sys.argv[sys.argv.index("--rows") + 1] if "--rows" in sys.argv else None
    run_dir = os.path.join(DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    om = lib096.load_origins_manifest()
    indexes, rows = {}, []
    for entry in lib096.load_manifest()["entries"]:
        if entry["set"] == "cc":
            continue  # sample truth, wrong family 0 under both rules
        doc = json.load(open(os.path.join(run_dir, "reports", entry["id"] + ".json")))
        if doc["libraryDiscovery"] is None:
            continue
        slice_ = doc["toolchain"]["slice"]
        if slice_ not in indexes:
            indexes[slice_] = score.load_index(pipeline["indexDir"], slice_, pipeline["indexId"])
        rows += rows_for(entry, doc, lib096.load_truth(entry), lib096.load_origins(entry, om), indexes[slice_])
    agg = collections.defaultdict(collections.Counter)
    for r in rows:
        agg[r["set"]][r["cause"]] += r["bytes"]
        agg[r["set"]]["TOTAL"] += r["bytes"]
    for s in sorted(agg):
        print(s, dict(agg[s].most_common()))
    if rows_out:
        with open(rows_out, "w") as fh:
            for r in sorted(rows, key=lambda r: -r["bytes"]):
                fh.write(json.dumps(r) + "\n")


if __name__ == "__main__":
    main()
