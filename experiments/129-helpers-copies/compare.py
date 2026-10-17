"""exp129: per set, P / F0.5 (and deltas vs off) of the helper_prints and terser_slices labels, from committed
summaries. usage: python3 -I compare.py"""
import json
import os

RES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../096-library-matching-score/results")
CORPORA = ["dev", "apps", "bundlers", "pop", "remin", "test", "test2"]
ARMS = ["off", "seed", "credit", "auto", "auto-seed", "auto-credit"]


def label(c, arm):
    base = {"off": "exp127-off", "auto": "exp127-auto20"}.get(arm, "exp129-" + arm)
    if c == "test2" and arm == "off":
        return "test2-exp124-r5"
    if c == "test2" and arm == "auto":
        return None
    return base if c == "dev" else f"{c}-{base}"


def main():
    print("| set | schema | " + " | ".join(f"{a} P / F0.5" for a in ARMS) + " |")
    print("|---" * (len(ARMS) + 2) + "|")
    for c in CORPORA:
        sums = {}
        for a in ARMS:
            lb = label(c, a)
            p = os.path.join(RES, lb, "summary.json") if lb else None
            sums[a] = json.load(open(p)) if p and os.path.exists(p) else None
        schema = sums["off"]["schema"]
        for s in sorted(sums["off"]["sets"]):
            cells = []
            o = sums["off"]["sets"][s]
            for a in ARMS:
                if sums[a] is None:
                    cells.append("=off (exp127)" if a == "auto" else "-")
                    continue
                r = sums[a]["sets"][s]
                cell = f"{100 * r['precision']:.2f} / {100 * r['f05']:.2f}"
                if a != "off":
                    cell += f" ({100 * (r['precision'] - o['precision']):+.3f} / {100 * (r['f05'] - o['f05']):+.3f})"
                cells.append(cell)
            print(f"| {s} | {schema.split('/')[-1]} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
