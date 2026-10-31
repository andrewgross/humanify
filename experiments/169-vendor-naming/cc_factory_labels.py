#!/usr/bin/env python3
"""exp169 phase 0 (cc): the labelled sample units (/work/cc-library-truth,
never committed) that fall inside a vendored CommonJS factory's raw span,
per label, from one names-mode run's diagnostics (eraRestoration.factoryFiles).
Prints totals; with --rows also every non-library unit (label, bytes,
vendor file, the labeller's one-line reason). Output is CC-derived: keep it
under /work, never in the repo.

usage: python3 -I cc_factory_labels.py <diag.json> [--rows]
"""
import bisect
import collections
import json
import sys

LABELS = "/work/cc-library-truth/2.1.216/labels.jsonl"


def main():
    er = json.load(open(sys.argv[1]))["eraRestoration"]
    fac = sorted((s, e, p) for (s, e), p in er["factoryFiles"])
    starts = [f[0] for f in fac]
    units = [json.loads(l) for l in open(LABELS)]
    tot = collections.Counter()
    inside = collections.Counter()
    rows = []
    for u in units:
        kind = "library" if u["label"].startswith("library") else u["label"]
        tot[kind] += u["bytes"]
        k = bisect.bisect_right(starts, u["start"]) - 1
        if k >= 0 and u["end"] <= fac[k][1]:
            inside[kind] += u["bytes"]
            if kind != "library":
                rows.append((kind, u["bytes"], fac[k][2], u.get("reason", "")))
    print(json.dumps({"labelledBytes": tot, "insideFactories": inside,
                      "factories": len(fac),
                      "factoryBytes": sum(e - s for s, e, _ in fac)}, indent=1))
    if "--rows" in sys.argv:
        for r in rows:
            print(json.dumps(r))


if __name__ == "__main__":
    main()
