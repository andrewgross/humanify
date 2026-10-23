"""exp141: P / R / F0.5 (both scopes, %) of main's defaults, terser auto + exp139's keys, and the candidate over
every DEV corpus (never test / test2 / remin2), from the committed summaries; deltas against main's defaults and the
flip bar for the candidate: precision >= main, F0.5 >= main - 0.002 points, app claimed <= main's (remin3's main
already claims app code on both sets). --json writes the rows; --identical also checks the candidate's per-bundle
cards against terser + keys on every set (totals and scopes).

usage: python3 -I table141.py [--json out.json] [--identical]"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RESULTS = os.path.join(HERE, "../../096-library-matching-score/results")
# (main defaults, terser auto + exp139 keys, candidate) per corpus
STEMS = [("exp138-helper", "exp139-both", "exp141-x"),
         ("exp138-helper-apps", "exp139-both-apps", "exp141-x-apps"),
         ("exp138-helper-bundlers", "exp139-both-bundlers", "exp141-x-bundlers"),
         ("pop-exp138-helper", "pop-exp139-both", "pop-exp141-x"),
         ("remin-exp138-helper", "remin-exp139-both", "remin-exp141-x"),
         ("remin3-exp141-main", "remin3-exp141-both", "remin3-exp141-x")]


def load(label):
    return json.load(open(os.path.join(RESULTS, label, "summary.json")))["sets"]


def cards(label):
    d = os.path.join(RESULTS, label, "cards")
    out = {}
    for f in sorted(os.listdir(d)):
        c = json.load(open(os.path.join(d, f)))
        out[c["entry"]] = (c["totals"], c["scopes"])
    return out


def main():
    a = sys.argv[1:]
    out = a[a.index("--json") + 1] if "--json" in a else None
    rows = []
    print("| set | main defaults P / R / F0.5 | terser auto + exp139 keys | + terser_anchor = all | d terser+keys P / F0.5"
          " | d candidate P / F0.5 | wrong KB main / keys / cand | bar |")
    print("|---|--:|--:|--:|--:|--:|--:|---|")
    for stems in STEMS:
        sets = [load(s) for s in stems]
        for name, base in sorted(sets[0].items()):
            rs = [s[name] for s in sets]
            last = rs[-1]
            ok = (last["precision"] >= base["precision"] and 100 * (last["f05"] - base["f05"]) >= -0.002
                  and last["appClaimedBytes"] <= base["appClaimedBytes"])
            cells = " | ".join(f"{100 * r['precision']:.3f} / {100 * r['recall']:.2f} / {100 * r['f05']:.3f}" for r in rs)
            ds = " | ".join(f"{100 * (r['precision'] - base['precision']):+.3f} / {100 * (r['f05'] - base['f05']):+.3f}"
                            for r in rs[1:])
            wr = " / ".join(f"{(r['wrongFamilyBytes'] + r['appClaimedBytes']) / 1024:.1f}" for r in rs)
            print(f"| {name} | {cells} | {ds} | {wr} | {'PASS' if ok else 'FAIL'} |")
            rows.append({"set": name, **{s: {k: r[k] for k in ("precision", "recall", "f05", "wrongFamilyBytes",
                                                                "namedBytes", "appClaimedBytes")}
                                         for s, r in zip(("main", "terserKeys", "candidate"), rs)}, "bar": ok})
    if "--identical" in a:
        for stems in STEMS:
            k, c = cards(stems[1]), cards(stems[2])
            same = sum(1 for e in c if k.get(e) == c[e])
            print(f"{stems[2]}: {same}/{len(c)} cards identical to {stems[1]}")
    if out:
        json.dump(rows, open(out, "w"), indent=1)


if __name__ == "__main__":
    main()
