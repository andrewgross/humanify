"""exp153 simulation: each call-context variant's first-order effect on a label's per-set score, from the census.

usage: python3 -I simulate.py <census-<corpus>.json> <baseline summary.json> [...more pairs] > simulation.json

First order only: a decision the census sees flips, nothing propagates from it (a new seed's DOWN / MODULE claims, a
dropped seed's stranded propagation and the veto's second round are not simulated: the built variant is measured).

Signals: fit = every callee with a print held; rowed = the same over the 20-node row floor; tree = the call-tree
hash, which only confirms; known / foreign = the exp155 scoped fits over the flat `edges/2` callee-print table
(an unheld callee the index knows nowhere, or the family holds anywhere, is skipped instead of counting against;
SILENT never drops a kept seed in the support variant, and the refute variant is the kill side of a veto).

Variants, per signal:
  tie-break     a tie the signal resolves to one family: a claimed unit's verdict becomes the resolved family's
                (a tie whose pick was dropped stays dropped: `tieUnclaimedUpper` counts what it could add)
  support-or    + the position-window drops it confirms, + the isolation-veto drops it confirms
  veto-or       + the isolation-veto drops it confirms
  support       support-or, and - every weak kept seed (under 100 nodes) it does not confirm, and - every kept
                index seed it does not confirm (the veto's neighbour test replaced)
  relax-floor   + the 20-29-node units it confirms for exactly one family (at most 3 owners)
  relax-common  + the too-common units (more than 3 owners) it confirms for exactly one family
  relax         both relaxers
  refute        - every kept index seed it refutes (a callee the family's rows lack): a guard, for reference
"""
import json
import sys

SIGNALS = ("fit", "rowed", "tree", "known", "foreign")
VARIANTS = ("tie-break", "support-or", "veto-or", "support", "relax-floor", "relax-common", "relax", "refute")


def cell(block, key, what="bytes"):
    return (block or {}).get(key, {}).get(what, 0)


def deltas(c, sig):
    """Per variant: {right, wrong, app} byte deltas (and units) from one set's census block."""
    out = {}

    def d():
        return {"right": 0, "wrong": 0, "app": 0, "units": 0}

    def add(acc, verdict, b, n, sign=1):
        if verdict in ("right", "wrong", "app"):
            acc[verdict] += sign * b
            acc["units"] += sign * n

    def rows(cat):
        return [(k.split(":"), v) for k, v in (c.get(cat) or {}).items() if k.split(":")[0] == sig]

    tie = d()
    tie_upper = d()
    for parts, v in rows("tie"):
        _, today, got, claimed = parts
        today = today[len("today-"):]
        if got == "unresolved":
            continue
        if claimed == "claimed":
            if got != today:
                add(tie, today, v["bytes"], v["units"], -1)
                add(tie, got, v["bytes"], v["units"])
        else:
            add(tie_upper, got, v["bytes"], v["units"])
    out["tie-break"] = tie
    out["tieUnclaimedUpper"] = tie_upper
    plus_support, plus_veto = d(), d()
    for parts, v in rows("support"):
        if parts[1] == "confirm":
            add(plus_support, parts[2], v["bytes"], v["units"])
    for parts, v in rows("veto"):
        if parts[1] == "confirm":
            add(plus_veto, parts[2], v["bytes"], v["units"])
    out["veto-or"] = plus_veto
    out["support-or"] = {k: plus_support[k] + plus_veto[k] for k in plus_veto}
    minus = d()
    for parts, v in rows("claim"):
        # a scoped signal's SILENCE (and its NONE, an index without the
        # flat table) is not evidence against a kept seed (exp155): the
        # DOWN-support rule drops only what the signal REFUTES
        drop = parts[1] == "refute" if sig in ("known", "foreign") else parts[1] != "confirm"
        if drop:
            add(minus, parts[2], v["bytes"], v["units"], -1)
    out["support"] = {k: out["support-or"][k] + minus[k] for k in minus}
    for name, cat in (("relax-floor", "floor"), ("relax-common", "common")):
        acc = d()
        for parts, v in rows(cat):
            add(acc, parts[1], v["bytes"], v["units"])
        out[name] = acc
    out["relax"] = {k: out["relax-floor"][k] + out["relax-common"][k] for k in acc}
    ref = d()
    for parts, v in rows("claim"):
        if parts[1] == "refute":
            add(ref, parts[2], v["bytes"], v["units"], -1)
    out["refute"] = ref
    return out


def score(named, wrong, app, lib):
    p = named / (named + wrong + app) if named + wrong + app else 0.0
    r = named / lib if lib else 0.0
    f = 1.25 * p * r / (0.25 * p + r) if p + r else 0.0
    return p, r, f


def main():
    args = sys.argv[1:]
    out = {"note": __doc__.split("\n\n")[1], "sets": {}}
    for census_path, summary_path in zip(args[::2], args[1::2]):
        census = json.load(open(census_path))
        summary = json.load(open(summary_path))
        for st, c in census["sets"].items():
            base = summary["sets"][st]
            named, wrong, app, lib = (base["namedBytes"], base["wrongFamilyBytes"], base["appClaimedBytes"],
                                      base["libraryBytes"])
            p0, r0, f0 = score(named, wrong, app, lib)
            row = {"baseline": {"precision": round(p0, 6), "recall": round(r0, 6), "f05": round(f0, 6),
                                "wrongKB": round(wrong / 1024, 1), "appBytes": app}}
            for sig in SIGNALS:
                ds = deltas(c, sig)
                for var in VARIANTS + ("tieUnclaimedUpper",):
                    dd = ds[var]
                    p, r, f = score(named + dd["right"], wrong + dd["wrong"], app + dd["app"], lib)
                    row[f"{var}/{sig}"] = {"dRightKB": round(dd["right"] / 1024, 1),
                                           "dWrongKB": round(dd["wrong"] / 1024, 1), "dAppBytes": dd["app"],
                                           "dUnits": dd["units"], "precision": round(p, 6), "recall": round(r, 6),
                                           "f05": round(f, 6), "dP": round(p - p0, 6), "dF05": round(f - f0, 6)}
            out["sets"][st] = row
    print(json.dumps(out, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
