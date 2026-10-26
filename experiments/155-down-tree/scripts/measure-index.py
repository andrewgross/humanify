"""exp155 index measurement: size delta, flat-table coverage and rowless share.

usage: python3 -I measure-index.py <exp152 index dir> <exp155 index dir>

Reports, per slice: the two directories' total bytes, the callee-prints tables' bytes,
and (from the edge rows both share) the flat table's keys — how many distinct callee
prints it answers for, and how many of those own no unit row of their own (below the
20-node row floor, or recorded only inside another package's OUT-OF list).
"""
import json
import os
import sys


def du(path):
    return sum(os.path.getsize(os.path.join(r, f)) for r, _, fs in os.walk(path) for f in fs)


def stats(ix):
    sls = sorted(f[len("edges."):-len(".jsonl")] for f in os.listdir(ix)
                 if f.startswith("edges.") and f.endswith(".jsonl"))
    out = {}
    for sl in sls:
        rowed = set()
        with open(os.path.join(ix, f"units.{sl}.jsonl")) as fh:
            for line in fh:
                rowed.add(line[9:25])
        keys = set()
        with open(os.path.join(ix, f"edges.{sl}.jsonl")) as fh:
            for line in fh:
                r = json.loads(line)
                keys.update(r["inside"])
                keys.update(c["hash"] for c in r["outOf"])
        cp = os.path.join(ix, f"callee-prints.{sl}.bin")
        out[sl] = {
            "calleePrintKeys": len(keys),
            "rowlessKeys": len(keys - rowed),
            "rowlessShare": round(len(keys - rowed) / len(keys), 4) if keys else 0,
            "calleePrintsBinBytes": os.path.getsize(cp) if os.path.exists(cp) else None,
        }
    out["totalBytes"] = du(ix)
    return out


def main():
    old, new = sys.argv[1], sys.argv[2]
    o, n = stats(old), stats(new)
    print(json.dumps({"exp152": o, "exp155": n,
                      "deltaBytes": n["totalBytes"] - o["totalBytes"]}, indent=1))
    for sl in sorted(k for k in n if k != "totalBytes"):
        print(f"slice {sl}: flat {n[sl]['calleePrintKeys']} keys "
              f"({n[sl]['rowlessShare']:.1%} rowless), "
              f"bin {n[sl]['calleePrintsBinBytes'] / 1e6:.1f} MB, "
              f"index +{(n['totalBytes'] - o['totalBytes']) / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
