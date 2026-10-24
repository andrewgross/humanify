"""exp149: the README's simulation table from sim.py's JSON: per set, KB fixed / broken and projected precision
change for chosen rules (scope `claimed`: the vote only re-decides ties the run claimed).

usage: python3 -I table.py <sim.json> <rule> ...     (rule as `<evidence>:<rule>`, e.g. excl:unique+ctx)"""
import json
import sys


def main():
    d = json.load(open(sys.argv[1]))
    rules = sys.argv[2:]
    print("| set | " + " | ".join(f"{r}: fixed / broken KB, d P" for r in rules) + " |")
    print("| --- | " + " | ".join("---:" for _ in rules) + " |")
    tot = {r: [0.0, 0.0, 0, 0] for r in rules}
    for s in sorted(d):
        cells = []
        for r in rules:
            x = d[s]["rules"][f"{r}:claimed"]
            t = tot[r]
            t[0] += x["fixedKB"]
            t[1] += x["brokenKB"]
            t[2] += x["dP"] < -1e-9
            t[3] += x["dP"] > 1e-9
            cells.append(f"{x['fixedKB']:.1f} / {x['brokenKB']:.1f}, {x['dP']:+.4f}")
        print(f"| {s} | " + " | ".join(cells) + " |")
    print("| **all** | " + " | ".join(
        f"**{t[0]:.1f} / {t[1]:.1f}**, down {t[2]} / up {t[3]} of {len(d)}" for t in tot.values()) + " |")


if __name__ == "__main__":
    main()
