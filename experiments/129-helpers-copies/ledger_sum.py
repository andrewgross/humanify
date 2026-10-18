"""exp129: per corpus, the scorer 11 -> 12 ledger summed over every label (bytes per class transition and kind),
and the labels with the largest right->wrong. usage: python3 -I ledger_sum.py <ledger-*.json>..."""
import collections
import json
import sys


def main():
    for p in sys.argv[1:]:
        d = json.load(open(p))
        tot, worst = collections.Counter(), collections.Counter()
        for label, r in d.items():
            for s, c in r["perSet"].items():
                for k, v in c.items():
                    if ":" in k:
                        tot[k] += v
                    if k == "GOOD->WRONG":
                        worst[label] += v
        corpus = next(iter(d.values()))["corpus"] if d else "?"
        print(f"{corpus} ({len(d)} labels): " + ", ".join(f"{k} {v:,.0f}" for k, v in sorted(tot.items())))
        if worst:
            print("   largest GOOD->WRONG: " + ", ".join(f"{k} {v:,.0f}" for k, v in worst.most_common(3)))


if __name__ == "__main__":
    main()
