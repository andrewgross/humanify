"""Print census.py's JSON as one markdown table per corpus (KB; recall points vs the corpus's library bytes)."""
import json
import sys

ORDER = ["0 (no hit)", "(0,10)", "[10,20)", "[20,30)", "[30,40)", "[40,50)", ">=50 (guard refused)"]


def kb(x):
    return f"{x / 1000:,.0f}"


for path in sys.argv[1:]:
    for set_, d in json.load(open(path)).items():
        print(f"\n{set_}: library {kb(d['libraryBytes'])} KB, recall {100 * d['recall']:.2f}%, "
              f"precision {100 * d['precision']:.3f}%, validation {d['validation']}")
        print("| cover | library unclaimed KB | unit right / wrong / app KB | +recall pts (unit) "
              "| function right / wrong / app KB | +recall pts (function) |")
        print("| --- | --: | --: | --: | --: | --: |")
        for b in ORDER:
            r = d["buckets"][b]
            u, f = r["unit"], r["function"]
            print(f"| {b} | {kb(r['libraryUnclaimedBytes'])} | {kb(u['right'])} / {kb(u['wrong'])} / {kb(u['app'])} "
                  f"| {r['recallPointsUnit']:.2f} | {kb(f['right'])} / {kb(f['wrong'])} / {kb(f['app'])} "
                  f"| {r['recallPointsFunction']:.2f} |")
