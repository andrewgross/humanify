"""exp144: for each (bundle, package) group the presence condition would drop, the packages claimed immediately
before and after the package's claimed span in source order (the flanks), in the NEW label. Tallies right / wrong
KB by flank shape: 'same' (both flanks one other package Q), 'mixed', 'none'. Names bundles only in --list.
usage: python3 -I flanks.py <rows.json> <corpus>=<base>:<new> ... [--list]"""
import bisect
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402


def rep(corpus, lab, entry, cache={}):
    k = (corpus, lab, entry)
    if k not in cache:
        lib096.use(corpus)
        cache.clear()
        cache[k] = json.load(open(os.path.join(lib096.DATA, "results", lab, "reports", entry + ".json")))[
            "libraryDiscovery"]
    return cache[k]


def main():
    rows = json.load(open(sys.argv[1]))
    labs = {a.split("=")[0]: a.split("=")[1].split(":") for a in sys.argv[2:] if "=" in a}
    g = collections.defaultdict(collections.Counter)
    for r in rows:
        if r["kind"] in ("gainedRight", "newWrong") and r["new"]:
            g[(r["corpus"], r["set"], r["entry"], r["new"])][r["kind"]] += r["bytes"]
    tally = collections.defaultdict(collections.Counter)
    lst = []
    for (corpus, s, entry, pkg), c in sorted(g.items()):
        base, new = labs[corpus]
        pb = {p["package"]: p for p in rep(corpus, base, entry)["packages"]}
        if pb.get(pkg, {}).get("seedUnits", 0) > 0:
            continue
        cl = sorted(rep(corpus, new, entry)["claims"], key=lambda x: x["start"])
        mine = [i for i, x in enumerate(cl) if x["package"] == pkg]
        lo, hi = mine[0], mine[-1]
        before = next((cl[i]["package"] for i in range(lo - 1, -1, -1) if cl[i]["package"] != pkg), None)
        after = next((cl[i]["package"] for i in range(hi + 1, len(cl)) if cl[i]["package"] != pkg), None)
        shape = "same" if before and before == after else "none" if not before and not after else "mixed"
        tally[s][shape + "Right"] += c["gainedRight"]
        tally[s][shape + "Wrong"] += c["newWrong"]
        lst.append((s, entry, pkg, before, after, round(c["gainedRight"] / 1024, 1), round(c["newWrong"] / 1024, 1)))
    kb = lambda v: round(v / 1024, 1)  # noqa: E731
    print("| set | same-flank right / wrong KB | mixed right / wrong | none right / wrong |")
    for s, t in sorted(tally.items()):
        print(f"| {s} | " + " | ".join(f"{kb(t[k + 'Right'])} / {kb(t[k + 'Wrong'])}" for k in ("same", "mixed", "none")) + " |")
    if "--list" in sys.argv:
        for x in sorted(lst, key=lambda x: -(x[5] + x[6]))[:40]:
            print(*x)


if __name__ == "__main__":
    main()
