"""exp102: which claims a rule removed or added, judged by the truth. usage: <venv python> -I claimdiff.py <base>
<label> [--top N]. Per changed statement unit (claimed in one label and not the other, or claimed differently):
the base and new claim, the truth's verdict on each, bytes; grouped by (change, claimed package, truth label)."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
from lib096 import DATA  # noqa: E402

GOOD = ("right", "family", "origin")


def claims(label, eid, key):
    doc = json.load(open(os.path.join(DATA, "results", label, "reports", eid + ".json")))
    rep = doc["libraryDiscovery"] or {}
    return {key[(c["start"], c["end"])]: c for c in rep.get("claims", []) if (c["start"], c["end"]) in key}, rep


def main():
    base, label = sys.argv[1], sys.argv[2]
    top = int(sys.argv[sys.argv.index("--top") + 1]) if "--top" in sys.argv else 25
    om = lib096.load_origins_manifest()
    groups = collections.Counter()
    tally = collections.Counter()
    for entry in lib096.load_manifest()["entries"]:
        if entry["set"] == "cc":
            continue
        doc = json.load(open(os.path.join(DATA, "results", base, "reports", entry["id"] + ".json")))
        if doc["libraryDiscovery"] is None:
            continue
        units = score.run_units(doc["libraryDiscovery"])
        st = score.statement_units(units)
        key = {(u["s"], u["e"]): u["i"] for u in st}
        by_i = {u["i"]: u for u in st}
        a, _ = claims(base, entry["id"], key)
        b, _ = claims(label, entry["id"], key)
        if a == b:
            continue
        truth = lib096.load_truth(entry)
        labels = score.label_units(truth, units, lib096.load_origins(entry, om))
        app = truth["appPackage"]
        for i in set(a) | set(b):
            ca, cb = a.get(i), b.get(i)
            pa, pb = ca and ca["package"], cb and cb["package"]
            if pa == pb:
                continue
            va = ca and score.judge_origin(pa, labels[i], app)[0]
            vb = cb and score.judge_origin(pb, labels[i], app)[0]
            n = by_i[i]["b"]
            kind = "removed" if cb is None else "added" if ca is None else "changed"
            groups[(entry["set"], kind, pa or pb, va if cb is None else vb, labels[i]["label"],
                    (ca or cb)["rule"])] += n
            tally[(entry["set"], kind, "GOOD" if (va if cb is None else vb) in GOOD else
                   "WRONG" if (va if cb is None else vb) == "wrongFamily" else "unjudged")] += n
    for k, v in sorted(tally.items()):
        print(f"{v:>9,}  {k}")
    print()
    for k, v in groups.most_common(top):
        print(f"{v:>9,}  {k}")


if __name__ == "__main__":
    main()
