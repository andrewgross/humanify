"""exp118 step 2: how much of the held-out NOT-INDEXED miss bytes each generic population would cover, in AGGREGATE.

usage: python3 -I census.py <data dir> <misses.json> <out.json> [<population.txt> for the density table]

Populations past the fetched ranked lists (dependents top-50k/100k, downloads 50k-100k) are judged by THRESHOLD:
the rank's count (thresholds.json, read off the ranked list's page at that rank) against each miss package's own
ecosyste.ms count. Their cost is an upper bound (N new packages).

Reads populations.json (populations.py: built from public npm metadata and the index only) and the private misses
file (misses.py). For the maintainer and scope policies a miss package's own public npm metadata (private/ dir:
fetched for the miss packages, never written to the repo) decides membership. Prints and writes ONLY aggregates:
byte shares, package counts, cost estimates. Never a held-out package name.

Cost model (stated, from measured builds):
  versions   = new packages x 5.48 (exp108: 34,766 pins / 6,343 new roots at year-last12+major4)
  index size = versions x 69 KB (k2-all: 6.4 GB / 92,237 owner-versions)
  build      = versions x 35 core-s / 30 cores (exp108 Bun/esbuild 8.9 core-s/version + exp113 webpack/rollup/
               literals 25.6 core-s/version)
"""
import datetime
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from populations import PSEUDO  # noqa: E402

sys.path.insert(0, os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                 "../../101-index-coverage/scripts")))
import tail_pins  # noqa: E402

ASOF = datetime.datetime.fromisoformat("2026-10-11T23:59:59.999+00:00")
DENSITIES = ["year-last12+major4", "year-last6+major2", "year-last3+major1", "year-last1+major1"]

PINS_PER_ROOT = 34766 / 6343
KB_PER_VERSION = 6.4e6 / 92237
CORE_S_PER_VERSION = 35.0
CORES = 30


def jl(p):
    with open(p) as f:
        return json.load(f)


def fn(name):
    return name.replace("/", "__") + ".json"


def miss_people(data, name, versions):
    out = set()
    for v in versions:
        p = os.path.join(data, "private", "ver", fn(f"{name}@{v}"))
        if os.path.exists(p):
            m = jl(p).get("body") or {}
            for x in m.get("maintainers") or []:
                if isinstance(x, dict) and x.get("name"):
                    out.add(x["name"].lower())
    p = os.path.join(data, "private", "eco", fn(name))
    if os.path.exists(p):
        out |= {x.lower() for x in jl(p).get("maintainers") or [] if x}
    return out - PSEUDO


def cost(n):
    v = n * PINS_PER_ROOT
    return {"packages": n, "versionsEst": round(v), "indexGBEst": round(v * KB_PER_VERSION / 1e6, 2),
            "buildHoursEst30c": round(v * CORE_S_PER_VERSION / CORES / 3600, 1)}


def main():
    data, misses_p, out = sys.argv[1], sys.argv[2], sys.argv[3]
    P = jl(os.path.join(data, "populations.json"))
    M = jl(misses_p)
    lib = M["libraryBytes"]
    a = {p: sum(vs.values()) for p, vs in M["a"].items()}
    A = sum(a.values())
    largest = max(a, key=a.get)
    scopes = set(P["scopes"])
    maint = P["maintainers"]  # maintainer -> number of indexed packages they maintain
    member = {k: set(v) for k, v in P["populations"].items()}
    member["scopes"] = {p for p in a if p.startswith("@") and p.split("/")[0] in scopes}
    people = {p: miss_people(data, p, M["a"][p]) for p in a}
    for k in (1, 3, 10, 30):
        member[f"maintainers(>={k} indexed pkgs)"] = {p for p in a if any(maint.get(x, 0) >= k for x in people[p])}
    eco = {p: jl(os.path.join(data, "private", "eco", fn(p))) for p in a}
    thr = jl(os.path.join(data, "thresholds.json"))
    upper = {}
    for key, lab, n in (("dependent_packages_count@rank50000", "dependents-top50k(threshold)", 50000),
                        ("dependent_packages_count@rank100000", "dependents-top100k(threshold)", 100000)):
        member[lab] = {p for p in a if (eco[p].get("dependents") or 0) >= thr[key]}
        upper[lab] = n
    t50 = P["meta"]["downloadsAtRank"]["50000"]
    member["downloads-50k-100k(threshold)"] = {p for p in a if thr["downloads@rank100000"] <= (
        eco[p].get("downloads") or 0) < t50}
    upper["downloads-50k-100k(threshold)"] = 50000
    rows = {}
    for k in sorted(member):
        cov = [p for p in a if p in member[k]]
        b = sum(a[p] for p in cov)
        b2 = sum(a[p] for p in cov if p != largest)
        row = {"coveredBytes": b, "shareOfNotIndexed": round(b / A, 4), "pointsOfLibrary": round(100 * b / lib, 2),
               "pointsOfLibraryExLargest": round(100 * b2 / lib, 2), "missPackagesCovered": len(cov)}
        if k in P["populations"]:
            row["cost"] = cost(len(P["populations"][k]))
        elif k in upper:
            row["cost"] = cost(upper[k])
            row["cost"]["upperBound"] = True
        rows[k] = row
    res = {"label": M["label"], "libraryBytes": lib, "notIndexedBytes": A,
           "notIndexedPointsOfLibrary": round(100 * A / lib, 2), "notIndexedPackages": len(a),
           "largestPackagePointsOfLibrary": round(100 * a[largest] / lib, 2),
           "meta": P["meta"], "policies": rows,
           "costModel": {"pinsPerRoot": round(PINS_PER_ROOT, 2), "kbPerVersion": round(KB_PER_VERSION, 1),
                         "coreSecondsPerVersion": CORE_S_PER_VERSION, "cores": CORES}}
    if len(sys.argv) > 4:
        pop = {x.strip() for x in open(sys.argv[4]) if x.strip()}
        dens = {}
        for pol in ["exact-any-version"] + DENSITIES:
            b = b2 = 0
            for p, vs in M["a"].items():
                if p not in pop:
                    continue
                if pol == "exact-any-version":
                    pins = set(vs)
                else:
                    pins = set(tail_pins.pick(jl(os.path.join(data, "private", "registry", fn(p))), ASOF, pol))
                for v, n in vs.items():
                    if v in pins:
                        b += n
                        b2 += n * (p != largest)
            dens[pol] = {"pointsOfLibrary": round(100 * b / lib, 2), "pointsOfLibraryExLargest": round(100 * b2 / lib, 2)}
        res["densityOnPopulation"] = {"population": os.path.basename(sys.argv[4]), "packages": len(pop),
                                      "byDensity": dens}
    with open(out, "w") as f:
        json.dump(res, f, indent=1, sort_keys=True)
        f.write("\n")
    print(f"not indexed: {res['notIndexedPointsOfLibrary']} pts of library bytes, {len(a)} pkgs; "
          f"largest single package {res['largestPackagePointsOfLibrary']} pts")
    print(f"{'policy':28} {'pts':>6} {'ex-top1':>7} {'share':>6} {'pkgs':>4} {'+pkgs':>8} {'+vers':>8} {'GB':>6} {'h@30':>6}")
    for k, r in rows.items():
        c = r.get("cost", {})
        print(f"{k:28} {r['pointsOfLibrary']:6.2f} {r['pointsOfLibraryExLargest']:7.2f} {r['shareOfNotIndexed']:6.3f} "
              f"{r['missPackagesCovered']:4d} {c.get('packages', '-'):>8} {c.get('versionsEst', '-'):>8} "
              f"{c.get('indexGBEst', '-'):>6} {c.get('buildHoursEst30c', '-'):>6}")
    for pol, r in res.get("densityOnPopulation", {}).get("byDensity", {}).items():
        print(f"density {pol:22} {r['pointsOfLibrary']:6.2f} {r['pointsOfLibraryExLargest']:7.2f}")


if __name__ == "__main__":
    main()
