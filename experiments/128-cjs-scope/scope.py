"""exp128: the factory scope per set, read from scorer-11 summaries (test / test2 included: aggregates only).

usage: python3 -I scope.py <label>...
       python3 -I scope.py --md <base label>:<candidate label>...   (the README's markdown table)
Per set: library bytes in the statement scope (scorers 1-9's whole scope) and in the factory scope (bundled CommonJS
modules), the factory share of library bytes, and P / R / F0.5 in each scope and over both (the scorer-11 headline)."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RESULTS = os.path.join(HERE, "../096-library-matching-score/results")


def pct(x):
    return f"{100 * x:6.2f}"


def md(pairs):
    print("| set | factory share | statement P / R / F0.5 (base) | all P / R / F0.5 (base) | factory P / R / F0.5 "
          "(cand) | all P / R / F0.5 (cand) | statement F0.5 (cand) | app B (cand) |")
    print("|---|--:|--:|--:|--:|--:|--:|--:|")
    for pair in pairs:
        a, b = pair.split(":")
        sa = json.load(open(os.path.join(RESULTS, a, "summary.json")))["sets"]
        sb = json.load(open(os.path.join(RESULTS, b, "summary.json")))["sets"]
        for name, ra in sa.items():
            rb = sb[name]
            st, fa = ra["scopes"]["statement"], ra["scopes"]["factory"]
            fb = rb["scopes"]["factory"]
            lib = st["libraryBytes"] + fa["libraryBytes"]

            def t(r):
                return f"{100 * r['precision']:.2f} / {100 * r['recall']:.2f} / {100 * r['f05']:.2f}"
            print(f"| {name} | {pct(fa['libraryBytes'] / lib if lib else 0).strip()}% | {t(st)} | {t(ra)} | "
                  f"{t(fb) if fb['libraryBytes'] else '-'} | {t(rb)} | {100 * rb['scopes']['statement']['f05']:.2f} | "
                  f"{rb['appClaimedBytes']} |")


def main():
    if sys.argv[1:2] == ["--md"]:
        return md(sys.argv[2:])
    print(f"{'label':28s} {'set':16s} {'stmt MB':>8s} {'fac MB':>8s} {'fac%':>6s} | {'stmt P':>6s} {'R':>6s} {'F0.5':>6s} | "
          f"{'fac P':>6s} {'R':>6s} {'F0.5':>6s} | {'all P':>6s} {'R':>6s} {'F0.5':>6s} | fac wrong B  app B")
    for label in sys.argv[1:]:
        s = json.load(open(os.path.join(RESULTS, label, "summary.json")))
        if not s["schema"].endswith("/11"):
            raise SystemExit(f"{label}: scored by {s['schema']}, not scorer 11")
        for name, row in s["sets"].items():
            st, fa = row["scopes"]["statement"], row["scopes"]["factory"]
            lib = st["libraryBytes"] + fa["libraryBytes"]
            print(f"{label:28s} {name:16s} {st['libraryBytes'] / 1e6:8.2f} {fa['libraryBytes'] / 1e6:8.2f} "
                  f"{pct(fa['libraryBytes'] / lib if lib else 0)} | {pct(st['precision'])} {pct(st['recall'])} "
                  f"{pct(st['f05'])} | {pct(fa['precision'])} {pct(fa['recall'])} {pct(fa['f05'])} | "
                  f"{pct(row['precision'])} {pct(row['recall'])} {pct(row['f05'])} | {fa['wrongFamilyBytes']:11d} "
                  f"{row['appClaimedBytes']:6d}" + ("  (factory scope not judged: sample truth)" if name == "cc" else ""))


if __name__ == "__main__":
    main()
