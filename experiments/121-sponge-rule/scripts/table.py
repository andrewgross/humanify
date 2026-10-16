"""exp121: per set P / R / F0.5 (summary.json) for k2-all (before), k2-all-dl25k and candidate labels, with each
candidate's dF0.5 vs k2-all. usage: python3 -I table.py <before> <dl25k> <cand>... (labels of ONE corpus)"""
import json
import os
import sys

RES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../096-library-matching-score/results")


def load(label):
    return json.load(open(os.path.join(RES, label, "summary.json")))["sets"]


def main():
    labels = sys.argv[1:]
    sums = [load(x) for x in labels]
    print("set | " + " | ".join(labels))
    for s in sums[0]:
        cells = []
        for i, S in enumerate(sums):
            r = S.get(s)
            if r is None:
                cells.append("-")
                continue
            c = f"{100 * r['precision']:.2f} / {100 * r['recall']:.2f} / **{100 * r['f05']:.2f}**"
            if i >= 1:
                c += f" ({100 * (r['f05'] - sums[0][s]['f05']):+.2f}; wrong {r['wrongFamilyBytes'] / 1e3:,.0f} KB)"
            else:
                c += f" (wrong {r['wrongFamilyBytes'] / 1e3:,.0f} KB)"
            if r["appClaimedBytes"]:
                c += f" APP {r['appClaimedBytes']}"
            cells.append(c)
        print(f"{s} | " + " | ".join(cells))


if __name__ == "__main__":
    main()
