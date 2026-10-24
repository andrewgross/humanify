"""exp149 census: main's LOST ties (named wrong; the truth's family is among the tied families) and WON ties (named
right; another family tied), and what a bundle-level vote does to each: resolved right, resolved wrong, or
unresolved (the vote does not decide, so today's rule stands). Aggregates only (KB per set).

With --carriers <file> (exp148's carrier packages, one per line) a third table: units the run left UNCLAIMED whose
tie involves a carrier package (on the carrier index: the originals the carriers made ambiguous), and what each vote
would name them.

usage: python3 -I census.py <tag> [--carriers <file>] <ties.jsonl> ...    (rules as in sim.py)"""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from sim import cls, pick  # noqa: E402

RULES = (("claim", "unique"), ("excl", "unique"), ("excl", "unique+ctx"), ("claim", "plural"), ("excl", "plural"))


def main():
    tag, args = sys.argv[1], sys.argv[2:]
    carriers = set()
    if args and args[0] == "--carriers":
        carriers = {x.strip() for x in open(args[1]) if x.strip()}
        args = args[2:]
    agg = collections.defaultdict(collections.Counter)
    for p in args:
        for line in open(p):
            r = json.loads(line)
            base = cls(r["verdict"])
            if base == "unclaimed" and carriers and any(e["pkg"] in carriers for e in r["ev"].values()):
                kind = "unclaimed-carrier"
            elif r["truthFam"] not in r["ev"] or base not in ("named", "wrong"):
                continue
            else:
                kind = "lost" if base == "wrong" else "won"
            s, b = r["set"], r["bytes"]
            agg[(s, kind)]["total"] += b
            for ev, rule in RULES:
                f = pick(r["ev"], ev, rule, r["claimFam"])
                if f is None:
                    out = "unresolved"
                else:
                    out = "right" if cls(r["ev"][f]["verdict"]) == "named" else "wrong"
                agg[(s, kind)][f"{ev}:{rule}:{out}"] += b
    sets = sorted({k[0] for k in agg})
    for kind in ("lost", "won") + (("unclaimed-carrier",) if carriers else ()):
        print(f"\n## {tag}: {kind} ties, KB (right / wrong / unresolved under each vote)")
        print("| set | total | " + " | ".join(f"{e}:{r}" for e, r in RULES) + " |")
        print("| --- | ---: | " + " | ".join("---:" for _ in RULES) + " |")
        T = collections.Counter()
        for s in sets + ["**all**"]:
            c = agg[(s, kind)] if s != "**all**" else T
            if s != "**all**":
                T.update(c)
            cells = [f"{c[f'{e}:{r}:right'] / 1024:.1f} / {c[f'{e}:{r}:wrong'] / 1024:.1f} / "
                     f"{c[f'{e}:{r}:unresolved'] / 1024:.1f}" for e, r in RULES]
            print(f"| {s} | {c['total'] / 1024:.1f} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
