"""exp106: every claim of a label judged by the CURRENT scorer (exp096 score.py: origins, the truth audit, the
derived families), for reading what a matcher change removed or added. DEV corpus only (never the test split).

usage: <venv python> -I claims.py wrong <label> [--top N] [--set S]   wrong-family / app bytes grouped by
                                                                      (set, claimed, truth, rule, evidence)
       <venv python> -I claims.py diff <base> <label> [--top N]       claims removed / added / changed, by verdict
       <venv python> -I claims.py rows <label> <out.jsonl>            one row per claimed statement unit
"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402

GOOD = ("right", "family", "origin")
_CTX = {}



def entry_claims(label, entry, om, cm, units_from=None):
    doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")))
    rep = doc["libraryDiscovery"]
    if rep is None:
        return {}
    ctx = _CTX.get(entry["id"])
    if ctx is None:
        units = score.run_units(rep)
        st = score.statement_units(units)
        truth = lib096.load_truth(entry)
        labels = score.label_units(truth, units, lib096.load_origins(entry, om),
                                   truth_audit.Audit(entry, truth, cm))
        ctx = ({u["i"]: u for u in st}, {(u["s"], u["e"]): u["i"] for u in st}, labels, truth["appPackage"])
        _CTX[entry["id"]] = ctx
    by_i, key, labels, app = ctx
    out = {}
    for c in rep.get("claims", []):
        i = key.get((c["start"], c["end"]))
        if i is None:
            continue
        v = score.judge_origin(c["package"], labels[i], app)[0]
        ev = (c.get("evidence") or {}).get("kind", "")
        out[i] = {"entry": entry["id"], "set": entry["set"], "start": c["start"], "end": c["end"],
                  "bytes": by_i[i]["b"], "package": c["package"], "rule": c["rule"], "round": c["round"],
                  "evidence": ev, "verdict": v, "truth": labels[i]["label"], "origin": labels[i].get("origin")}
    return out


def entries():
    return [e for e in lib096.load_manifest()["entries"] if e["set"] != "cc"]


def manifests():
    return lib096.load_origins_manifest(), truth_audit.load_copies_manifest()


def cmd_rows(label, out):
    om, cm = manifests()
    with open(out, "w") as fh:
        for e in entries():
            for r in entry_claims(label, e, om, cm).values():
                fh.write(json.dumps(r) + "\n")


def cmd_wrong(label, top, only_set):
    om, cm = manifests()
    g = collections.Counter()
    tot = collections.Counter()
    for e in entries():
        if only_set and e["set"] != only_set:
            continue
        for r in entry_claims(label, e, om, cm).values():
            tot[(r["set"], r["verdict"])] += r["bytes"]
            if r["verdict"] in ("wrongFamily", "app"):
                g[(r["set"], r["package"], r["truth"], r["rule"], r["evidence"])] += r["bytes"]
    for k, v in sorted(tot.items()):
        print(f"{v:>10,}  {k}")
    print()
    for k, v in g.most_common(top):
        print(f"{v:>9,}  {k}")


def cmd_diff(base, label, top):
    om, cm = manifests()
    groups = collections.Counter()
    tally = collections.Counter()
    for e in entries():
        a = entry_claims(base, e, om, cm)
        b = entry_claims(label, e, om, cm)
        for i in set(a) | set(b):
            ca, cb = a.get(i), b.get(i)
            if ca and cb and ca["package"] == cb["package"]:
                continue
            kind = "removed" if cb is None else "added" if ca is None else "changed"
            r = cb or ca
            v = (ca if cb is None else cb)["verdict"]
            cls = "GOOD" if v in GOOD else "WRONG" if v in ("wrongFamily", "app") else "unjudged"
            if kind == "changed":
                cls = f"{ca['verdict']}->{cb['verdict']}"
            tally[(e["set"], kind, cls)] += r["bytes"]
            groups[(e["set"], kind, (ca or cb)["package"], cb["package"] if kind == "changed" else "",
                    v, r["truth"], r["rule"], r["evidence"])] += r["bytes"]
    for k, v in sorted(tally.items()):
        print(f"{v:>9,}  {k}")
    print()
    for k, v in groups.most_common(top):
        print(f"{v:>9,}  {k}")


def main():
    a = sys.argv[1:]
    top = int(a[a.index("--top") + 1]) if "--top" in a else 40
    only = a[a.index("--set") + 1] if "--set" in a else None
    if a[0] == "rows":
        cmd_rows(a[1], a[2])
    elif a[0] == "wrong":
        cmd_wrong(a[1], top, only)
    elif a[0] == "diff":
        cmd_diff(a[1], a[2], top)
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
