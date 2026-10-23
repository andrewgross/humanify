"""exp139: P / R / F0.5 (both scopes, %) of four stems over every corpus, from the committed summaries (test / test2:
aggregates only), deltas of each against the first, and the flip bar for the last: precision >= base, F0.5 >= base
- 0.002 points, app claimed 0. --json writes the rows.

usage: python3 -I table139.py <base> <stem>... [--json out.json]"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RESULTS = os.path.join(HERE, "../../096-library-matching-score/results")
FORMS = ["{}", "{}-apps", "{}-bundlers", "pop-{}", "remin-{}", "test-{}", "test2-{}"]


def load(label):
    p = os.path.join(RESULTS, label, "summary.json")
    return json.load(open(p))["sets"] if os.path.exists(p) else None


def main():
    args = sys.argv[1:]
    out = args[args.index("--json") + 1] if "--json" in args else None
    stems = [a for i, a in enumerate(args) if a != "--json" and (i == 0 or args[i - 1] != "--json")]
    rows = []
    print("| set | " + " | ".join(f"{s} P / R / F0.5" for s in stems) + " | "
          + " | ".join(f"d {s} P / F0.5" for s in stems[1:]) + " | bar (last) |")
    print("|---|" + "--:|" * (2 * len(stems) - 1) + "---|")
    for form in FORMS:
        sets = [load(form.format(s)) for s in stems]
        if any(x is None for x in sets):
            continue
        for name, base in sets[0].items():
            rs = [s[name] for s in sets]
            last = rs[-1]
            ok = (last["precision"] >= base["precision"] and 100 * (last["f05"] - base["f05"]) >= -0.002
                  and last["appClaimedBytes"] == 0)
            cells = " | ".join(f"{100 * r['precision']:.3f} / {100 * r['recall']:.2f} / {100 * r['f05']:.3f}" for r in rs)
            ds = " | ".join(f"{100 * (r['precision'] - base['precision']):+.3f} / {100 * (r['f05'] - base['f05']):+.3f}"
                            for r in rs[1:])
            print(f"| {name} | {cells} | {ds} | {'PASS' if ok else 'fail'} |")
            rows.append({"set": name, **{s: {k: r[k] for k in ("precision", "recall", "f05", "wrongFamilyBytes",
                                                                "namedBytes", "appClaimedBytes")}
                                         for s, r in zip(stems, rs)}, "bar": ok})
    if out:
        json.dump(rows, open(out, "w"), indent=1)


if __name__ == "__main__":
    main()
