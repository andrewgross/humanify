"""exp145: per-set deltas of two DEV labels from their cards (all sets the labels hold).

usage: python3 pairs145.py <results dir> <base label> <cand label>
Prints per set: apps, dNamed KB, dWrong KB (wrong family + app claimed), added / removed wrong KB, apps with wrong
up, the largest single-app add, and the marginal precision of the delta (1 - dWrong/dNamed)."""
import collections
import glob
import json
import os
import sys

JUDGE = ("test-", "test2-", "remin2-")


def load(results, label):
    if label.startswith(JUDGE):
        raise SystemExit("judge labels are never read")
    out = collections.defaultdict(dict)
    for p in glob.glob(os.path.join(results, label, "cards", "*.json")):
        c = json.load(open(p))
        s = c["set"]
        if s.startswith(JUDGE):
            raise SystemExit("judge set in a dev label?")
        t = c.get("totals") or {}
        out[s][c["entry"]] = (t.get("namedBytes", 0), t.get("wrongFamilyBytes", 0) + t.get("appClaimedBytes", 0))
    return out


def main(results, base, cand):
    b, c = load(results, base), load(results, cand)
    for s in sorted(b):
        apps = sorted(set(b[s]) & set(c.get(s, {})))
        dn = sum(c[s][a][0] - b[s][a][0] for a in apps)
        dws = [c[s][a][1] - b[s][a][1] for a in apps]
        nb = sum(b[s][a][0] for a in apps)
        wb = sum(b[s][a][1] for a in apps)
        mp = 1 - sum(dws) / dn if dn > 0 else float("nan")
        print(json.dumps({"set": s, "apps": len(apps), "P_base": round(nb / (nb + wb), 6) if nb else None,
                          "dNamedKB": round(dn / 1e3, 1), "dWrongKB": round(sum(dws) / 1e3, 2),
                          "addedKB": round(sum(max(0, x) for x in dws) / 1e3, 2),
                          "removedKB": round(-sum(min(0, x) for x in dws) / 1e3, 2),
                          "appsUp": sum(1 for x in dws if x > 0), "maxAppAddKB": round(max(dws + [0]) / 1e3, 2),
                          "marginalP": round(mp, 6)}))


if __name__ == "__main__":
    main(*sys.argv[1:4])
