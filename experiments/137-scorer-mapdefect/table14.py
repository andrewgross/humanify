"""exp137: per-set table of label stems, scorer 13 (the committed summaries at a git ref) vs scorer 14 (the
summaries in the working tree), every corpus (test / test2: their committed aggregates only). Headline = both
scopes.

usage: python3 -I table14.py <scorer-13 git ref> <stem> [<stem> ...] [--json out]"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../.."))
RESULTS = "experiments/096-library-matching-score/results"
FORMS = ["{}", "{}-apps", "{}-bundlers", "pop-{}", "remin-{}", "test-{}", "test2-{}"]


def at_ref(ref, label):
    r = subprocess.run(["git", "-C", REPO, "show", f"{ref}:{RESULTS}/{label}/summary.json"], capture_output=True,
                       text=True)
    return json.loads(r.stdout) if r.returncode == 0 else None


def now(label):
    p = os.path.join(REPO, RESULTS, label, "summary.json")
    return json.load(open(p)) if os.path.exists(p) else None


def cell(r):
    return f"{100 * r['precision']:.2f} / {100 * r['recall']:.2f} / {100 * r['f05']:.2f}"


def main():
    argv = sys.argv[1:]
    out = argv[argv.index("--json") + 1] if "--json" in argv else None
    args = [a for i, a in enumerate(argv) if a != "--json" and (i == 0 or argv[i - 1] != "--json")]
    ref, stems = args[0], args[1:]
    rows = []
    print("| label | set | scorer 13 P / R / F0.5 | scorer 14 P / R / F0.5 | dP | dR | dF0.5 | wrong KB 13 -> 14 |")
    print("|---|---|--:|--:|--:|--:|--:|--:|")
    for stem in stems:
        for form in FORMS:
            label = form.format(stem)
            a, b = at_ref(ref, label), now(label)
            if a is None or b is None:
                continue
            if b["schema"] != "exp096-score/14":
                raise SystemExit(f"{label}: not re-scored under scorer 14")
            for name, ra in sorted(a["sets"].items()):
                rb = b["sets"][name]
                d = {k: 100 * (rb[k] - ra[k]) for k in ("precision", "recall", "f05")}
                rows.append({"label": label, "set": name, "s13": {k: ra[k] for k in ("precision", "recall", "f05")},
                             "s14": {k: rb[k] for k in ("precision", "recall", "f05")},
                             "wrong13": ra["wrongFamilyBytes"], "wrong14": rb["wrongFamilyBytes"],
                             "named13": ra["namedBytes"], "named14": rb["namedBytes"]})
                print(f"| {label} | {name} | {cell(ra)} | {cell(rb)} | {d['precision']:+.3f} | {d['recall']:+.3f} | "
                      f"{d['f05']:+.3f} | {ra['wrongFamilyBytes'] / 1e3:.1f} -> {rb['wrongFamilyBytes'] / 1e3:.1f} |")
    if out:
        open(out, "w").write(json.dumps(rows, indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
