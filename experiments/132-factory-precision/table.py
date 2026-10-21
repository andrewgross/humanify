"""exp132: the README's per-set table, read from the committed summaries (test / test2: aggregates only).

usage: python3 -I table.py <base>:<factory_bodies>:<candidate> ...
Per set, over both scopes (the scorer-11+ headline): P / R / F0.5 of today's shipped defaults (base), of
`factory_bodies` alone, and of the candidate; the factory scope's P for the two cut runs; the bar verdict (candidate
P >= base P, F0.5 up, app claimed 0)."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RESULTS = os.path.join(HERE, "../096-library-matching-score/results")


def t(r):
    return f"{100 * r['precision']:.2f} / {100 * r['recall']:.2f} / {100 * r['f05']:.2f}"


def tradeoff(quads):
    """base:fb:cand:variant -> P / F0.5 of each, the variant's bar verdict."""
    print("| set | base P / F0.5 | factory_bodies | candidate | + factory_whole_seed | bar (cand / +whole) |")
    print("|---|--:|--:|--:|--:|---|")
    for quad in quads:
        sets = [json.load(open(os.path.join(RESULTS, x, "summary.json")))["sets"] for x in quad.split(":")]
        for name, ra in sets[0].items():
            rows = [s[name] for s in sets]
            if all(r["precision"] == ra["precision"] and r["f05"] == ra["f05"] for r in rows):
                continue

            def bar(r):
                return "PASS" if r["precision"] >= ra["precision"] and r["f05"] >= ra["f05"] else "FAIL"
            print(f"| {name} | " + " | ".join(f"{100 * r['precision']:.3f} / {100 * r['f05']:.2f}" for r in rows)
                  + f" | {bar(rows[2])} / {bar(rows[3])} |")


def main():
    if sys.argv[1:2] == ["--tradeoff"]:
        return tradeoff(sys.argv[2:])
    print("| set | base P / R / F0.5 | factory_bodies P / R / F0.5 | candidate P / R / F0.5 | factory-scope P "
          "(fb -> cand) | bar |")
    print("|---|--:|--:|--:|--:|---|")
    for triple in sys.argv[1:]:
        a, b, c = (json.load(open(os.path.join(RESULTS, x, "summary.json")))["sets"] for x in triple.split(":"))
        for name, ra in a.items():
            rb, rc = b[name], c[name]
            fb, fc = rb["scopes"]["factory"], rc["scopes"]["factory"]
            fac = (f"{100 * fb['precision']:.2f} -> {100 * fc['precision']:.2f}" if fc["libraryBytes"] else "-")
            ok = rc["precision"] >= ra["precision"] and rc["f05"] >= ra["f05"] and rc["appClaimedBytes"] == 0
            same = rc["precision"] == ra["precision"] and rc["f05"] == ra["f05"]
            print(f"| {name} | {t(ra)} | {t(rb)} | {t(rc)} | {fac} | "
                  f"{'= (no factories)' if same else 'PASS' if ok else 'FAIL'} |")


if __name__ == "__main__":
    main()
