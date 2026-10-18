"""exp129: per set, P / F0.5 (and deltas vs off) of the helper_prints and terser_slices labels, from committed
summaries. usage: python3 -I compare.py [final [statement|factory]]   (final: the k2-all-r5-origin labels; default:
the k2-all-r5h ones; a scope name reads that scope's block instead of the card totals, which cover both scopes)"""
import sys
import json
import os

RES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../096-library-matching-score/results")
CORPORA = ["dev", "apps", "bundlers", "pop", "remin", "test", "test2"]
ARMS = ["off", "seed", "credit", "auto", "auto-seed", "auto-credit"]


FINAL_ARMS = ["off", "seed", "auto", "auto-seed"]


def final_label(c, arm):
    if arm == "off":
        return "exp130-origin" if c == "dev" else (f"exp130-origin-{c}" if c in ("apps", "bundlers")
                                                   else f"{c}-exp130-origin")
    return f"exp129o-{arm}" if c == "dev" else f"{c}-exp129o-{arm}"


def label(c, arm):
    base = {"off": "exp127-off", "auto": "exp127-auto20"}.get(arm, "exp129-" + arm)
    if c == "test2" and arm == "off":
        return "test2-exp124-r5"
    if c == "test2" and arm == "auto":
        return None
    return base if c == "dev" else f"{c}-{base}"


def main():
    global ARMS, label
    if sys.argv[1:2] == ["final"]:
        ARMS, label = FINAL_ARMS, final_label
    scope = sys.argv[2] if len(sys.argv) > 2 else None  # "statement" / "factory": that scope's block
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
            def pick(d):
                return d["scopes"][scope] if scope and "scopes" in d else d
            o = pick(sums["off"]["sets"][s])
            for a in ARMS:
                if sums[a] is None:
                    cells.append("=off (exp127)" if a == "auto" else "-")
                    continue
                r = pick(sums[a]["sets"][s])
                cell = f"{100 * r['precision']:.2f} / {100 * r['f05']:.2f}"
                if a != "off":
                    cell += f" ({100 * (r['precision'] - o['precision']):+.3f} / {100 * (r['f05'] - o['f05']):+.3f})"
                cells.append(cell)
            print(f"| {s} | {schema.split('/')[-1]} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
