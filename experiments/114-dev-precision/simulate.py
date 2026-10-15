"""exp114 lab: seed-level simulation of bundle-level seed rules over seedlab.py rows (no propagation: an estimate
for design only; the Rust run is the measurement). usage: <venv python> -I simulate.py <seeds.jsonl>..."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import score  # noqa: E402
import functools  # noqa: E402

SAME = functools.lru_cache(maxsize=None)(score.same_family)

GOOD = ("right", "family", "origin")


def fam_reps(pkgs):
    reps = []
    for p in sorted(pkgs):
        if not any(SAME(p, r) for r in reps):
            reps.append(p)
    return reps


def ok(pkg, r):
    return SAME(pkg, r["truth"]) or (r["origin"] and SAME(pkg, r["origin"]))


def fam_of(p, reps):
    for r in reps:
        if SAME(p, r):
            return r
    return p


def simulate(rows, variant):
    out = collections.Counter()
    by_entry = collections.defaultdict(list)
    for r in rows:
        if "accept" in r:
            by_entry[r["entry"]].append(r)
    for e, seeds in by_entry.items():
        allp = set()
        for s in seeds:
            allp.update(s["accept"]); allp.update(s["holders"]); allp.add(s["package"])
        reps = fam_reps(allp)
        fmap = {p: fam_of(p, reps) for p in allp}
        F = fmap.__getitem__
        uniq = collections.Counter()
        for s in seeds:
            if len({F(p) for p in s["accept"]}) == 1:
                uniq[F(s["package"])] += s["bytes"]
        carries = collections.defaultdict(set)
        for s in seeds:
            fams = {F(p) for p in (s["holders"] if variant.get("holders") else s["accept"])}
            for p in fams:
                carries[p] |= fams - {p}
        for s in seeds:
            v = s["verdict"] in GOOD
            vv = "good" if v else ("wrong" if s["verdict"] == "wrongFamily" else "other")
            new = s["package"]
            fams = {F(p) for p in s[variant.get("tieset", "tied")]}
            P = F(s["package"])
            if variant.get("tie") and len(fams) > 1:
                best = max(fams, key=lambda f: (uniq[f], f == P))
                if uniq[best] == 0:
                    new = None
                else:
                    new = best
            if variant.get("carrier") and new is not None:
                Pn = F(new)
                if any(uniq[q] > variant.get("ratio", 1) * uniq[Pn] for q in carries[Pn]):
                    new = None
            if new == s["package"]:
                continue
            if new is None:
                out[(s["set"], "dropped", vv)] += s["bytes"]
            else:
                nv = "good" if ok(new, s) else "wrong"
                out[(s["set"], f"{vv}->{nv}")] += s["bytes"]
    return out


def main():
    rows = []
    for p in sys.argv[1:]:
        rows += [json.loads(l) for l in open(p)]
    for name, var in [("tie", {"tie": 1}), ("tie-accept", {"tie": 1, "tieset": "accept"}),
                      ("carrier", {"carrier": 1}), ("carrier-holders", {"carrier": 1, "holders": 1}),
                      ("tie+carrier", {"tie": 1, "carrier": 1}),
                      ("tie+carrier-holders", {"tie": 1, "carrier": 1, "holders": 1}),
                      ("tie+carrier-holders r3", {"tie": 1, "carrier": 1, "holders": 1, "ratio": 3})]:
        o = simulate(rows, var)
        print("==", name)
        for k, v in sorted(o.items()):
            print(f"   {v:>10,}  {k}")


if __name__ == "__main__":
    main()
