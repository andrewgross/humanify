"""exp149: what the bundle shows on each side of a tie (ties.py rows), aggregates only.

For every LOST tie (the run named another family W; the truth's family T is among the tied families) and every WON
tie (the run named the truth's family; R = the best-evidenced other tied family), by evidence kind: does T show
other evidence in the bundle (> 0 bytes), does W / R, and which is larger; plus the context signal (T holds the code
in an index module no bigger than W's / R's). KB per set and pattern.

usage: python3 -I patterns.py <ties.jsonl> ..."""
import collections
import json
import sys

KINDS = ("held", "claim", "excl", "exclClaim")


def side(ev, k, t, w):
    a, b = ev[t][k], ev[w][k] if w in ev else 0
    return ("T" if a > 0 else "-") + ("W" if b > 0 else "-") + (">" if a > b else "<" if a < b else "=")


def main():
    agg = collections.defaultdict(collections.Counter)
    for p in sys.argv[1:]:
        for line in open(p):
            r = json.loads(line)
            ev, t = r["ev"], r["truthFam"]
            if t not in ev:
                continue
            if r["verdict"] == "wrongFamily":
                kind, w = "lost", r["claimFam"]
            elif r["verdict"] in ("right", "family", "origin") and r["claimFam"] == t:
                others = [k for k in ev if k != t]
                if not others:
                    continue
                kind, w = "won", max(others, key=lambda k: (ev[k]["excl"], ev[k]["claim"], k))
            else:
                continue
            s, b = r["set"], r["bytes"]
            for k in KINDS:
                agg[(s, kind, k)][side(ev, k, t, w)] += b
            ct, cw = ev[t].get("ctx"), (ev.get(w) or {}).get("ctx")
            agg[(s, kind, "ctx")]["T<=W" if ct is not None and cw is not None and ct <= cw else
                                  "T>W" if ct is not None and cw is not None else "?"] += b
            agg[(s, kind, "total")]["all"] += b
    for key in sorted(agg):
        c = agg[key]
        print(key[0], key[1], key[2], " ".join(f"{p}:{v / 1024:.1f}" for p, v in sorted(c.items(), key=lambda x: -x[1])))


if __name__ == "__main__":
    main()
