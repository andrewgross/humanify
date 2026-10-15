"""exp118 step 1: the HELD-OUT not-indexed ("a") miss bytes of one scored label, per package, written to a PRIVATE
file outside the repo (default /work/exp118-data/misses-<label>.json). Prints aggregates only.

The private file exists so the census (census.py) can test package MEMBERSHIP in generic, independently-built
population lists. Neither script ever prints, commits or selects by a held-out package name: every output of the
census is a byte share and a package count.

usage: <exp095 scale venv python> -I misses.py <label> [<out.json>]
"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.normpath(os.path.join(HERE, "../../101-index-coverage/scripts")))
import miss_buckets as mb  # noqa: E402


def main():
    label = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else f"/work/exp118-data/misses-{label}.json"
    captured, lib, _ = mb.run(label)
    cls = collections.Counter()
    a = collections.defaultdict(lambda: collections.Counter())
    for c, p, v, b in captured:
        cls[c] += b
        if c == "a":
            a[p][v or ""] += b
    res = {"label": label, "libraryBytes": lib, "missBytes": dict(cls),
           "a": {p: dict(vs) for p, vs in a.items()}}
    with open(out, "w") as f:
        json.dump(res, f, sort_keys=True)
    A = sum(sum(vs.values()) for vs in a.values())
    print(json.dumps({"label": label, "libraryBytes": lib, "missBytes": dict(sorted(cls.items())),
                      "aBytes": A, "aShareOfLibrary": round(A / lib, 4), "aPackages": len(a),
                      "aVersions": sum(len(vs) for vs in a.values())}, indent=1))


if __name__ == "__main__":
    main()
