"""exp142: base vs as-of-delta index, per set, from the labels' summary.json (scorer 14, both scopes).
usage: python3 -I table142.py <base stem> <new stem> [<out.json>]   (dev labels: <stem>, <stem>-apps,
<stem>-bundlers, pop-<stem>, remin-<stem>, remin3-<stem>; judge summaries test-/test2-/remin2-<stem> if present)
Bar: P >= base, F0.5 >= base - 0.002 (points), app claimed <= base."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RES = os.path.join(HERE, "../../096-library-matching-score/results")
DEV = ("{}", "{}-apps", "{}-bundlers", "pop-{}", "remin-{}", "remin3-{}")
JUDGE = ("test-{}", "test2-{}", "remin2-{}")


def sets(label):
    p = os.path.join(RES, label, "summary.json")
    return json.load(open(p))["sets"] if os.path.exists(p) else None


def main():
    base, new = sys.argv[1], sys.argv[2]
    rows = []
    print("| set | base P / R / F0.5 | as-of delta P / R / F0.5 | d P / d R / d F0.5 | wrong+app KB base / new | bar |")
    print("| --- | ---: | ---: | ---: | ---: | --- |")
    for group in (DEV, JUDGE):
        for pat in group:
            a, b = sets(pat.format(base)), sets(pat.format(new))
            if a is None or b is None:
                continue
            for s in sorted(a):
                x, y = a[s], b[s]
                f = lambda r: (100 * r["precision"], 100 * r["recall"], 100 * r["f05"])  # noqa: E731
                (p0, r0, f0), (p1, r1, f1) = f(x), f(y)
                w0 = (x["wrongFamilyBytes"] + x["appClaimedBytes"]) / 1024
                w1 = (y["wrongFamilyBytes"] + y["appClaimedBytes"]) / 1024
                ok = p1 >= p0 - 1e-9 and f1 >= f0 - 0.002 and y["appClaimedBytes"] <= x["appClaimedBytes"]
                rows.append({"set": s, "judge": group is JUDGE, "base": [p0, r0, f0], "new": [p1, r1, f1],
                             "wrongAppKB": [round(w0, 1), round(w1, 1)], "app": [x["appClaimedBytes"],
                                                                                  y["appClaimedBytes"]],
                             "bar": ok})
                print(f"| {s} | {p0:.3f} / {r0:.2f} / {f0:.3f} | {p1:.3f} / {r1:.2f} / {f1:.3f} | "
                      f"{p1 - p0:+.3f} / {r1 - r0:+.2f} / {f1 - f0:+.3f} | {w0:.1f} / {w1:.1f} | "
                      f"{'PASS' if ok else 'FAIL'} |")
    if len(sys.argv) > 3:
        json.dump(rows, open(sys.argv[3], "w"), indent=1)


if __name__ == "__main__":
    main()
