"""exp148 step 3 simulation: what adding only a SUBSET S of exp133's download-top-25k packages would do, read from
exp133's two scored labels (base = k2-all-r5-origin, dl = + every dl25k package), unit by unit (misses.py rows of
both labels, joined by entry and unit order; scorer 14 verdicts).

A unit whose verdict differs between the labels is attributed:
  - dl claims a candidate-only package P            -> the change is P's: kept iff P in S
  - else the truth's package T is candidate-only     -> the change is T's (T now indexed): kept iff T in S
  - else (base->base claim change, or lost to unclaimed with no candidate on either side) -> UNATTRIBUTED: reported
    both ways (kept = upper, reverted = lower)
Prints per set: dWrong KB, dNamed KB for S (lower/upper) and for all of dl25k.

usage: python3 -I sim133.py <base ix versions.json> <S.txt|ALL> <set>=<base.jsonl>,<dl.jsonl> ..."""
import collections
import json
import sys

NAMED = ("right", "family", "origin")


def load(path):
    per = collections.defaultdict(list)
    for line in open(path):
        r = json.loads(line)
        per[r["entry"]].append(r)
    return per


def main():
    base_pk = set(json.load(open(sys.argv[1])))
    s_arg = sys.argv[2]
    S = None if s_arg == "ALL" else {l.strip() for l in open(s_arg) if l.strip()}
    out = {}
    for arg in sys.argv[3:]:
        name, paths = arg.split("=", 1)
        bp, dp = paths.split(",")
        b, d = load(bp), load(dp)
        acc = collections.defaultdict(collections.Counter)
        for e, rows in b.items():
            drows = d.get(e, [])
            if len(drows) != len(rows):
                acc["?"]["entryMismatch"] += 1
                continue
            for rb, rd in zip(rows, drows):
                assert rb["bytes"] == rd["bytes"] and rb["pkg"] == rd["pkg"], e
                s = rb["set"]
                for k, r in (("base", rb), ("dl", rd)):
                    acc[s][k + "Wrong"] += r["bytes"] if r["verdict"] == "wrongFamily" else 0
                    acc[s][k + "Named"] += r["bytes"] if r["named"] else 0
                if rb["verdict"] == rd["verdict"] and rb["claimed"] == rd["claimed"]:
                    continue
                c = rd["claimed"]
                if c and c not in base_pk:
                    who, att = c, True
                elif rb["pkg"] not in base_pk:
                    who, att = rb["pkg"], True
                else:
                    who, att = None, False
                keep = (S is None or who in S) if att else None
                dw = (rd["verdict"] == "wrongFamily") - (rb["verdict"] == "wrongFamily")
                dn = rd["named"] - rb["named"]
                if att and keep:
                    acc[s]["sWrong"] += dw * rb["bytes"]
                    acc[s]["sNamed"] += dn * rb["bytes"]
                elif not att:
                    acc[s]["uWrong"] += dw * rb["bytes"]
                    acc[s]["uNamed"] += dn * rb["bytes"]
        for s, c in acc.items():
            if s == "?":
                continue
            out[s] = c
            k = lambda x: c[x] / 1024  # noqa: E731
            print(f"{s:14s} base wrong {k('baseWrong'):8.1f} named {k('baseNamed'):9.0f} | all-dl dWrong "
                  f"{k('dlWrong') - k('baseWrong'):+8.1f} dNamed {k('dlNamed') - k('baseNamed'):+8.1f} | S dWrong "
                  f"{k('sWrong'):+7.1f} (+unattr {k('uWrong'):+7.1f}) dNamed {k('sNamed'):+7.1f} (+unattr "
                  f"{k('uNamed'):+7.1f})")


if __name__ == "__main__":
    main()
