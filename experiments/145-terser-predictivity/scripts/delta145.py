"""exp145: unit-level deltas between two labels from judge13.py rows (DEV only; rows stay outside the repo).

usage: python3 delta145.py <rows.jsonl> [--set S]
Per set: bytes newly judged right / wrong (c0 not wrong -> c1 wrong family or app) / lost right, and the new-wrong
bytes per claimed family and per (bundle, family) event, largest first."""
import collections
import json
import sys

BAD = ("wrongFamily", "app")


def verdict(c):
    return (c or {}).get("verdict")


def main(argv):
    path = argv[0]
    only = argv[argv.index("--set") + 1] if "--set" in argv else None
    agg = collections.defaultdict(lambda: collections.Counter())
    fam = collections.defaultdict(collections.Counter)
    ev = collections.defaultdict(collections.Counter)
    for line in open(path):
        r = json.loads(line)
        s = r["set"]
        if only and s != only:
            continue
        v0, v1 = verdict(r.get("c0")), verdict(r.get("c1"))
        b = r["bytes"]
        if v1 in BAD and v0 not in BAD:
            agg[s]["newWrong"] += b
            p = r["c1"]["package"]
            fam[s][p] += b
            ev[s][(r["entry"], p)] += b
        elif v0 in BAD and v1 not in BAD:
            agg[s]["fixedWrong"] += b
        if v1 == "right" and v0 != "right":
            agg[s]["newRight"] += b
        if v0 == "right" and v1 != "right":
            agg[s]["lostRight"] += b
    for s in sorted(agg):
        tot = agg[s]["newWrong"]
        evs = sorted(ev[s].values(), reverse=True)
        print(json.dumps({"set": s, **{k: round(v / 1e3, 2) for k, v in agg[s].items()},
                          "events": len(evs), "top1": round(sum(evs[:1]) / tot, 3) if tot else 0,
                          "top3": round(sum(evs[:3]) / tot, 3) if tot else 0,
                          "top5": round(sum(evs[:5]) / tot, 3) if tot else 0,
                          "families": [(p, round(b / 1e3, 1)) for p, b in fam[s].most_common(6)]}))


if __name__ == "__main__":
    main(sys.argv[1:])
