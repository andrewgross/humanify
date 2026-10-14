"""exp112: what scorer 7 moved, in both directions: per set, bytes by (scorer-6 verdict -> scorer-7 verdict) and,
for the moved units, the origin evidence kind scorer 7 gave them. Aggregate only.

usage: python3 -I ledger.py <rows-s6.jsonl> <rows-s7.jsonl> [--samples N]   (rows.py with and without --s6)
--samples prints the largest moved (claimed, truth, origin, file) groups: dev corpora only."""
import collections
import json
import sys


def load(p):
    return {(r["entry"], r["start"], r["end"]): r for r in map(json.loads, open(p))}


def main():
    a, b = load(sys.argv[1]), load(sys.argv[2])
    samples = int(sys.argv[sys.argv.index("--samples") + 1]) if "--samples" in sys.argv else 0
    t = collections.defaultdict(collections.Counter)
    g = collections.defaultdict(collections.Counter)
    for k, rb in b.items():
        ra = a[k]
        if ra["verdict"] == rb["verdict"]:
            continue
        key = (ra["verdict"], rb["verdict"], rb.get("originEvidence"))
        t[rb["set"]][key] += rb["bytes"]
        g[(rb["set"], key)][(rb["package"], rb["truth"], rb.get("origin"), rb["truthFile"])] += rb["bytes"]
    for s in sorted(t):
        print(s)
        for k, v in t[s].most_common():
            print(f"  {v:>12,}  {k[0]} -> {k[1]}  ({k[2]})")
            for kk, vv in g[(s, k)].most_common(samples):
                print(f"        {vv:>10,}  {kk}")


if __name__ == "__main__":
    main()
