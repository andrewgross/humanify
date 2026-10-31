#!/usr/bin/env python3
"""exp172 phase 2: placement precision/recall of the app-module rule on
bun-dev, against the source-map truth.

The factory population and each factory's truth bytes come from the
phase-0 census (census.mjs rows: every extracted factory of exp166's
names-mode runs, raw span + truth bytes per label). A run label's
diagnostics (bundev-runs.sh) list the factories that run kept in the app
(`appModules`, raw declarator spans); a census factory counts as MOVED
when an app-module span lies inside its removal span.

App bytes = `<self>` + `<app>` truth bytes; library bytes = every other
labelled byte. Prints totals first, then the moved library bytes
itemized (the bar is ~0), then per-bundle rows for every bundle with app
bytes in factories or anything moved.

usage: python3 -I score.py <census.jsonl> <runs-dir> [--json out.json]
"""
import collections
import json
import os
import sys

APP = {"<self>", "<app>"}


def main():
    census, runs = sys.argv[1], sys.argv[2]
    out_json = sys.argv[sys.argv.index("--json") + 1] if "--json" in sys.argv else None
    rows = collections.defaultdict(list)
    for line in open(census):
        r = json.loads(line)
        rows[r["bundle"]].append(r)
    tot = collections.Counter()
    per_bundle = []
    wrong = []
    missing_runs = []
    for bundle in sorted(rows):
        diag = os.path.join(runs, bundle, "diag.json")
        if not os.path.exists(diag):
            missing_runs.append(bundle)
            continue
        d = json.load(open(diag))
        moved_spans = [(m["start"], m["end"], m["reason"]) for m in d.get("appModules") or []]
        b = collections.Counter()
        for r in rows[bundle]:
            truth = r.get("truth") or {}
            app = sum(v for k, v in truth.items() if k in APP)
            lib = sum(v for k, v in truth.items() if k not in APP)
            hit = [m for m in moved_spans if r["start"] <= m[0] and m[1] <= r["end"]]
            b["factories"] += 1
            b["appBytes"] += app
            b["libBytes"] += lib
            if hit:
                b["moved"] += 1
                b["movedAppBytes"] += app
                b["movedLibBytes"] += lib
                if lib:
                    # majority: the factory is library code, not a mapping
                    # sliver of an app module's wrapper
                    wrong.append({"libraryMajority": lib * 2 > app,
                        "bundle": bundle, "factory": r["factory"], "reason": hit[0][2],
                        "bytes": r["bytes"], "libBytes": lib, "appBytes": app,
                        "truth": truth, "vendorFileBefore": r["vendorFile"],
                    })
        b["unmatchedAppModules"] = len(moved_spans) - b["moved"]
        tot.update(b)
        if b["appBytes"] or b["moved"]:
            per_bundle.append({"bundle": bundle, **{k: b.get(k, 0) for k in ("factories", "appBytes", "libBytes", "moved", "movedAppBytes", "movedLibBytes", "unmatchedAppModules")}})
    recall = tot["movedAppBytes"] / tot["appBytes"] if tot["appBytes"] else 0
    precision = (tot["movedAppBytes"] / (tot["movedAppBytes"] + tot["movedLibBytes"])
                 if tot["movedAppBytes"] + tot["movedLibBytes"] else 1)
    print(f"TOTAL bundles scored {len(rows) - len(missing_runs)} (no run: {len(missing_runs)})")
    print(f"  factories {tot['factories']}, moved to the app {tot['moved']}")
    print(f"  app bytes in factories {tot['appBytes']}, moved {tot['movedAppBytes']} "
          f"(recall {recall:.4f})")
    print(f"  library bytes moved into the app {tot['movedLibBytes']} (precision {precision:.4f}); "
          f"library-majority factories moved: {sum(1 for w in wrong if w['libraryMajority'])}")
    print(f"  app-module spans with no census factory: {tot['unmatchedAppModules']}")
    print("library bytes moved, itemized:")
    for w in wrong:
        print(f"  {w['bundle']} {w['factory']} ({w['reason']}): {w['libBytes']} lib / "
              f"{w['appBytes']} app bytes, truth {w['truth']}, was {w['vendorFileBefore']}")
    print("per bundle (app bytes in factories, or anything moved):")
    for p in per_bundle:
        print(f"  {p['bundle']}: app {p['movedAppBytes']}/{p['appBytes']} moved, "
              f"lib moved {p['movedLibBytes']}, factories moved {p['moved']}/{p['factories']}")
    if missing_runs:
        print("no run:", " ".join(missing_runs))
    if out_json:
        with open(out_json, "w") as fh:
            json.dump({"totals": dict(tot), "recall": recall, "precision": precision,
                       "wrong": wrong, "perBundle": per_bundle, "missingRuns": missing_runs},
                      fh, indent=1, sort_keys=True)
            fh.write("\n")


if __name__ == "__main__":
    main()
