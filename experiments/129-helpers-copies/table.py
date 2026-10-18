"""exp129: one row per (set, label) from committed summaries: P / R / F0.5, wrong-family and app bytes.

usage: python3 -I table.py <label>... (a dev label without a prefix; <corpus>-<name> for the others)"""
import json
import os
import sys

RES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../096-library-matching-score/results")


def main():
    rows = []
    for label in sys.argv[1:]:
        p = os.path.join(RES, label, "summary.json")
        if not os.path.exists(p):
            print(f"{label}: no summary")
            continue
        d = json.load(open(p))
        for s, r in sorted(d["sets"].items()):
            rows.append((s, label, d["schema"], r["precision"], r["recall"], r["f05"], r["wrongFamilyBytes"],
                         r["appClaimedBytes"], r["namedBytes"]))
    rows.sort()
    print(f"{'set':16s} {'label':28s} {'P':>7s} {'R':>7s} {'F0.5':>7s} {'wrong':>9s} {'app':>5s} {'named':>11s}")
    for s, label, _, p, r, f, w, a, n in rows:
        print(f"{s:16s} {label:28s} {100 * p:7.2f} {100 * r:7.2f} {100 * f:7.2f} {w:9d} {a:5d} {n:11d}")


if __name__ == "__main__":
    main()
