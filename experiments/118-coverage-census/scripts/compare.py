"""exp118: per-set P / R / F0.5 (scorer cards' summary.json), base label vs candidate label, for any corpus.
usage: python3 -I compare.py <results dir> <base label> <cand label> [...more pairs]"""
import json
import os
import sys


def main():
    d = sys.argv[1]
    pairs = sys.argv[2:]
    for b, c in zip(pairs[::2], pairs[1::2]):
        B = json.load(open(os.path.join(d, b, "summary.json")))["sets"]
        C = json.load(open(os.path.join(d, c, "summary.json")))["sets"]
        for s in B:
            if s not in C:
                continue
            x, y = B[s], C[s]
            f = lambda r: f"{100 * r['precision']:6.2f} / {100 * r['recall']:6.2f} / {100 * r['f05']:6.2f}"
            print(f"{s:16s} {f(x)}  ->  {f(y)}   dF0.5 {100 * (y['f05'] - x['f05']):+6.2f}  dP {100 * (y['precision'] - x['precision']):+6.2f}"
                  f"  wrongKB {x['wrongFamilyBytes'] / 1e3:8.1f} -> {y['wrongFamilyBytes'] / 1e3:8.1f}  app {y['appClaimedBytes']}")


if __name__ == "__main__":
    main()
