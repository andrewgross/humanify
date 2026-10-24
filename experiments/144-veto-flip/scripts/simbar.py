"""exp144: a seed-level guard simulated on a NEW label's per-set cards: remove (dRight, dWrong) KB of the new
label's claims, report P / R / F0.5 vs the base label and the bar (P >= base, F0.5 >= base - 0.002 pts).
No propagation is modelled beyond what the rows carry. usage: python3 -I simbar.py <base stem> <new stem> <sim.json>
  sim.json: {"<set>": [dRightKB, dWrongKB], ...} (sets absent = unchanged)"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RES = os.path.join(HERE, "../../096-library-matching-score/results")
DEV = ("{}", "{}-apps", "{}-bundlers", "pop-{}", "remin-{}", "remin3-{}")


def sets(label):
    p = os.path.join(RES, label, "summary.json")
    return json.load(open(p))["sets"] if os.path.exists(p) else {}


def f05(p, r):
    return 1.25 * p * r / (0.25 * p + r) if p + r else 0.0


def main():
    base, new, sim = sys.argv[1], sys.argv[2], json.load(open(sys.argv[3]))
    print("| set | base P / F0.5 | new P / F0.5 | sim P / F0.5 | bar |")
    print("| --- | ---: | ---: | ---: | --- |")
    for pat in DEV:
        a, b = sets(pat.format(base)), sets(pat.format(new))
        for s in sorted(b):
            if s not in a:
                continue
            x, y = a[s], b[s]
            dr, dw = (v * 1024 for v in sim.get(s, [0, 0]))
            named = y["namedBytes"]
            right = y["precision"] * named
            right2, named2 = right - dr, named - dr - dw
            p2 = right2 / named2 if named2 else 0
            r2 = y["recall"] * right2 / right if right else 0
            f2 = f05(p2, r2)
            ok = p2 >= x["precision"] - 1e-9 and f2 >= x["f05"] - 0.00002
            print(f"| {s} | {100 * x['precision']:.3f} / {100 * x['f05']:.3f} | {100 * y['precision']:.3f} / "
                  f"{100 * y['f05']:.3f} | {100 * p2:.3f} / {100 * f2:.3f} | {'PASS' if ok else 'FAIL'} |")


if __name__ == "__main__":
    main()
