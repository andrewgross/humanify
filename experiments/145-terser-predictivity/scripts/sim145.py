"""exp145: simulate a generic per-(bundle, family) guard on a candidate's NEW claims (DEV rows only).

usage: python3 sim145.py <rows.jsonl> <set> <cand summary.json> <base summary.json> [--cap KB ...] [--ratio R ...]
A (bundle, family) EVENT = the candidate's claims of family F in bundle B on units the base did not give to F.
Guard (cap C bytes, ratio R): drop an event's claims when its bytes exceed C AND exceed R x the bytes the BASE
already gives F in that bundle (R = 0: only a family the base never claimed in B counts as new... see `new_only`).
First order: dropped claims become unclaimed (no propagation re-run). Prints P / R / F0.5 of base, candidate and
each guard, plus right / wrong bytes the guard drops."""
import collections
import json
import sys

BAD = ("wrongFamily", "app")


def f05(p, r):
    return 1.25 * p * r / (0.25 * p + r) if p + r else 0


def main(argv):
    rows, set_, cand, base = argv[:4]
    caps = [float(x) * 1e3 for x in argv[argv.index("--cap") + 1].split(",")] if "--cap" in argv else [1e3, 4e3, 16e3]
    ratios = [float(x) for x in argv[argv.index("--ratio") + 1].split(",")] if "--ratio" in argv else [0, 1, 4]
    halves = "--halves" in argv
    base_bytes = collections.Counter()
    ev = collections.defaultdict(lambda: [0, 0])  # (entry, fam) -> [right, wrong]
    for line in open(rows):
        r = json.loads(line)
        if r["set"] != set_:
            continue
        c0, c1 = r.get("c0") or {}, r.get("c1") or {}
        if c0.get("package"):
            base_bytes[(r["entry"], c0["package"])] += r["bytes"]
        p1 = c1.get("package")
        if p1 and p1 != c0.get("package"):
            v = c1.get("verdict")
            if v == "right":
                ev[(r["entry"], p1)][0] += r["bytes"]
            elif v in BAD:
                ev[(r["entry"], p1)][1] += r["bytes"]
    sc = json.load(open(cand))["sets"][set_]
    sb = json.load(open(base))["sets"][set_]

    def pr(s, dr=0, dw=0):
        named, wrong = s["namedBytes"] - dr, s["wrongFamilyBytes"] + s["appClaimedBytes"] - dw
        p = named / (named + wrong)
        rc = named / s["libraryBytes"]
        return p, rc

    def line(tag, p, rc, dr=0, dw=0):
        print(f"{tag:<26} P {100*p:8.3f}  R {100*rc:6.2f}  F0.5 {100*f05(p, rc):6.2f}  dropRightKB {dr/1e3:8.1f}  dropWrongKB {dw/1e3:6.1f}")

    print(f"== {set_}: {len(ev)} events, new right {sum(v[0] for v in ev.values())/1e3:.1f} KB, new wrong {sum(v[1] for v in ev.values())/1e3:.1f} KB")
    line("base", *pr(sb))
    line("candidate", *pr(sc))
    for cap in caps:
        for ratio in ratios:
            dr = dw = 0
            for (e, fam), (rt, wr) in ev.items():
                n = rt + wr
                if n > cap and n > ratio * base_bytes[(e, fam)] and (ratio > 0 or base_bytes[(e, fam)] == 0):
                    dr += rt
                    dw += wr
            line(f"cap {cap/1e3:g}KB ratio {ratio:g}", *pr(sc, dr, dw), dr, dw)


if __name__ == "__main__":
    main(sys.argv[1:])
