"""exp144: the (bundle, package) groups the presence condition would drop (package has no kept seed in the base
run), with right / wrong KB and the package's row in the NEW report (seeds, into-edges). Names bundles -> stdout only
(redirect outside the repo). usage: python3 -I absentgroups.py <rows.json> <corpus>=<base>:<new> ..."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402


def rep(corpus, lab, entry):
    lib096.use(corpus)
    return json.load(open(os.path.join(lib096.DATA, "results", lab, "reports", entry + ".json")))["libraryDiscovery"]


def main():
    rows = json.load(open(sys.argv[1]))
    labs = {a.split("=")[0]: a.split("=")[1].split(":") for a in sys.argv[2:]}
    g = collections.defaultdict(collections.Counter)
    for r in rows:
        if r["kind"] in ("gainedRight", "newWrong") and r["new"]:
            g[(r["corpus"], r["set"], r["entry"], r["new"])][r["kind"]] += r["bytes"]
    out = []
    for (corpus, s, entry, pkg), c in g.items():
        base, new = labs[corpus]
        pb = {p["package"]: p for p in rep(corpus, base, entry)["packages"]}
        if pb.get(pkg, {}).get("seedUnits", 0) > 0:
            continue
        pn = {p["package"]: p for p in rep(corpus, new, entry)["packages"]}.get(pkg, {})
        out.append((s, entry, pkg, round(c["gainedRight"] / 1024, 1), round(c["newWrong"] / 1024, 1),
                    pn.get("seedUnits"), pn.get("seedBytes"), pn.get("intoUnits"), pn.get("grownUnits"),
                    pb.get(pkg, {}).get("vetoedUnits")))
    out.sort(key=lambda x: -(x[3] + x[4]))
    print("set entry pkg rightKB wrongKB seedUnits seedBytes intoUnits grownUnits baseVetoedUnits")
    for o in out:
        print(*o)


if __name__ == "__main__":
    main()
