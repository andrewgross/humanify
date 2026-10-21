"""exp133: per-set table of two (or more) label stems over every corpus, read from the committed summaries
(test / test2: aggregates only). Headline = both scopes (scorer 11+). Bar column: F0.5 up AND precision >= the
first stem's.

usage: python3 -I table.py <base stem> <stem> [<stem> ...]"""
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
    stems = sys.argv[1:]
    head = " | ".join(f"{s} P / R / F0.5" for s in stems)
    print(f"| set | {head} | dP | dF0.5 | wrong KB (base -> last) | bar |")
    print("|---|" + "--:|" * len(stems) + "--:|--:|--:|---|")
    for form in FORMS:
        sets = [load(form.format(s)) for s in stems]
        if any(x is None for x in sets):
            continue
        for name, ra in sets[0].items():
            rows = [s[name] for s in sets]
            last = rows[-1]
            dp = 100 * (last["precision"] - ra["precision"])
            df = 100 * (last["f05"] - ra["f05"])
            ok = last["precision"] >= ra["precision"] and last["f05"] > ra["f05"] and last["appClaimedBytes"] == 0
            cells = " | ".join(f"{100 * r['precision']:.2f} / {100 * r['recall']:.2f} / {100 * r['f05']:.2f}"
                               for r in rows)
            wr = f"{ra['wrongFamilyBytes'] / 1e3:.1f} -> {last['wrongFamilyBytes'] / 1e3:.1f}"
            print(f"| {name} | {cells} | {dp:+.2f} | {df:+.2f} | {wr} | {'PASS' if ok else 'fail'} |")


if __name__ == "__main__":
    main()
