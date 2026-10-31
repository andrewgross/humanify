#!/usr/bin/env python3
"""exp169 phase 0: which `<self>` / `<app>` truth bytes sit inside the
vendored CommonJS factories on bun-dev, itemized per bundle, vendor file and
ORIGINAL source file (the truth's `sources` rows: [label, version, file]).

Reads exp166's names-mode stub runs (`/work/exp166-data/bundev/v2/<id>/diag.json`,
eraRestoration.factoryFiles = every vendored factory's raw span + file) and
the frozen bun-dev truth (lib096). Writes one JSON row per
(bundle, vendor file, source file) with the bytes and a 160-char excerpt of
the source-mapped code (for the hand read; bun-dev is public npm code).

usage: python3 -I self_in_factories.py <runs-dir> > rows.jsonl
"""
import bisect
import collections
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORER = os.path.join(HERE, "../096-library-matching-score")
sys.path.insert(0, SCORER)
import lib096  # noqa: E402

LABELS = {"<self>", "<app>"}


def main():
    runs = sys.argv[1]
    lib096.use("bun-dev")
    entries = {
        e["id"]: e
        for e in json.load(open(os.path.join(SCORER, "corpus-bun-dev.json")))["entries"]
    }
    for rid in sorted(os.listdir(runs)):
        diag = os.path.join(runs, rid, "diag.json")
        if not os.path.exists(diag) or rid not in entries:
            continue
        er = (json.load(open(diag)).get("eraRestoration")) or {}
        factories = er.get("factoryFiles") or []
        if not factories:
            continue
        e = entries[rid]
        t = json.load(gzip.open(lib096.truth_path(e), "rt"))
        segs, srcs = t["segments"], t["sources"]
        offs = [s[0] for s in segs]
        raw = open(e["files"]["bundle"]["origin"], "rb").read()
        for (s, end), path in factories:
            per = collections.defaultdict(lambda: [0, None])
            k = max(bisect.bisect_right(offs, s) - 1, 0)
            while k < len(segs) and segs[k][0] < end:
                a = max(segs[k][0], s)
                b = min(segs[k + 1][0] if k + 1 < len(segs) else end, end)
                src = srcs[segs[k][1]]
                if b > a and src[0] in LABELS:
                    row = per[(src[0], src[2])]
                    row[0] += b - a
                    if row[1] is None:
                        row[1] = a
                k += 1
            for (label, file), (n, first) in sorted(per.items()):
                print(json.dumps({
                    "bundle": rid,
                    "appPackage": t.get("appPackage"),
                    "vendorFile": path,
                    "factoryBytes": end - s,
                    "label": label,
                    "sourceFile": file,
                    "bytes": n,
                    "excerpt": raw[first:first + 160].decode("utf-8", "replace"),
                }))


if __name__ == "__main__":
    main()
