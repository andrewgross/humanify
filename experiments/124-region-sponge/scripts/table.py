"""exp124: P / R / F0.5 per set, baseline vs candidates (summary.json of each label; test/test2 summaries only).
usage: python3 -I table.py <baseline stem> <stem> [<stem> ...]"""
import json
import os
import sys

R = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../096-library-matching-score/results")
CORPORA = [("{}", None), ("{}-apps", None), ("{}-bundlers", None), ("pop-{}", None), ("test-{}", None), ("test2-{}", None)]


def sets(label):
    p = os.path.join(R, label, "summary.json")
    return json.load(open(p))["sets"] if os.path.exists(p) else {}


def main():
    stems = sys.argv[1:]
    print(f"{'set':22s}" + "".join(f"{s:>34s}" for s in stems))
    for pat, _ in CORPORA:
        base = sets(pat.format(stems[0]))
        for name in base:
            row = f"{pat.format('').strip('-') or 'dev'}:{name}"[:22].ljust(22)
            for s in stems:
                x = sets(pat.format(s)).get(name)
                if not x:
                    row += f"{'-':>34s}"
                    continue
                b = base[name]
                row += (f"  {100 * x['precision']:6.2f}/{100 * x['recall']:5.2f}/{100 * x['f05']:5.2f}"
                        f" ({100 * (x['f05'] - b['f05']):+5.2f}) a{x['appClaimedBytes']}")
            print(row)


if __name__ == "__main__":
    main()
