"""exp133: decompose the wrong-family bytes label c1 has and c0 does not (and the reverse), from a judge13.py file,
by mechanism columns only: set, scope, claim rule/evidence, whether c0 claimed the unit at all (and its verdict),
whether the truth carries an origin, and whether the wrong package is NEW (not claimed anywhere by c0 in the
corpus — a proxy for 'a newly indexed package'). Aggregates only; no package names printed unless --names (never
for held-out: it is refused for the heldout set).

usage: python3 -I wrongdiff.py <judge.jsonl> [--names SET]"""
import collections
import json
import sys


def main():
    path = sys.argv[1]
    names_set = sys.argv[sys.argv.index("--names") + 1] if "--names" in sys.argv else None
    if names_set == "heldout":
        raise SystemExit("REFUSED: no held-out package names")
    rows = [json.loads(l) for l in open(path)]
    c0_pkgs = collections.defaultdict(set)
    for r in rows:
        if "c0" in r:
            c0_pkgs[r["set"]].add(r["c0"]["package"])
    gained, lost = collections.Counter(), collections.Counter()
    by_pkg = collections.Counter()
    for r in rows:
        v0 = r.get("c0", {}).get("verdict", "unclaimed")
        v1 = r.get("c1", {}).get("verdict", "unclaimed")
        if v1 == "wrongFamily" and v0 != "wrongFamily":
            c = r["c1"]
            new = "newpkg" if c["package"] not in c0_pkgs[r["set"]] else "oldpkg"
            key = (r["set"], r["scope"], c["rule"], c["evidence"] or "-", "c0=" + v0, new,
                   "origin" if r.get("origin") else "no-origin")
            gained[key] += r["bytes"]
            if names_set == r["set"]:
                by_pkg[(c["package"], r["truth"], r.get("origin"))] += r["bytes"]
        if v0 == "wrongFamily" and v1 != "wrongFamily":
            lost[(r["set"], r["scope"], "c1=" + v1)] += r["bytes"]
    tot = collections.Counter()
    for k, b in gained.items():
        tot[k[0]] += b
    print("NEW wrong bytes (c1 wrong, c0 not), by set:", dict(tot))
    for k, b in gained.most_common(40):
        print(f"  {b / 1e3:9.1f} KB  " + "  ".join(k))
    print("wrong bytes c1 FIXED (c0 wrong, c1 not):")
    for k, b in lost.most_common(15):
        print(f"  {b / 1e3:9.1f} KB  " + "  ".join(k))
    if names_set:
        print(f"by package on {names_set} (claimed, truth, origin):")
        for k, b in by_pkg.most_common(30):
            print(f"  {b / 1e3:9.1f} KB  {k}")


if __name__ == "__main__":
    main()
