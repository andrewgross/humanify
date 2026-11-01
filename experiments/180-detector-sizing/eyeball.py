#!/usr/bin/env python3
"""exp180: eyeball the detector's false flags and misses on bun-dev (rule 1: never trust a match not looked at).

usage: <exp095 venv python> -I eyeball.py <pass dir> <data dir> <rule> <K> [n]

For the n most frequent FALSE flags (flagged, no unit of the family in the bundle) prints the bundle, the hit
print, its index owners and the bundle code it sits in (the unit's first 160 bytes, from the bundle file). Not for
cc or bun-held (cc truth stays private; bun-held is private).
"""
import collections
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.argv = [sys.argv[0], sys.argv[1], sys.argv[2], "/dev/null"] + sys.argv[3:]
sys.path.insert(0, HERE)
import simulate as sim  # noqa: E402

RULE, K = sys.argv[4], int(sys.argv[5])
N = int(sys.argv[6]) if len(sys.argv) > 6 else 8
REPORTS = "/work/exp150-data/bun-dev/frozen/results/exp167-names-cb64/reports"


def main():
    versions = json.load(open(os.path.join(sim.INDEX, "versions.json")))
    cands = sim.load_candidates()
    sim.RULES = {RULE: sim.RULES[RULE]}
    sim.KS = (K,)
    dets, _ = sim.build_detectors(cands)
    det = dets[(RULE, K)]
    x = json.load(gzip.open(os.path.join(sim.DATA, "x-bun-dev.json.gz"), "rt"))
    paths = dict(line.split() for line in open(os.path.join(sim.DATA, "bundles-bun-dev.txt")))
    index_keys = sim.fam_keys(versions)
    false = collections.defaultdict(list)
    for b in x["bundles"]:
        bb = sim.Bundle(b, index_keys, {})
        flagged, _ = sim.detect(bb, det, 1)
        present = sim.fam_keys(bb.present)
        for o in flagged:
            if (bb.app and sim.keys(o) & sim.keys(bb.app)) or sim.keys(o) & present:
                continue
            hits = [h for h in bb.prints if o in det.get(h, {})]
            false[o].append((b["id"], hits))
    print(f"{RULE}/K{K}: {sum(len(v) for v in false.values())} false flags, {len(false)} owners")
    for o, rows in sorted(false.items(), key=lambda kv: -len(kv[1]))[:N]:
        bid, hits = rows[0]
        rep = json.load(open(os.path.join(REPORTS, bid + ".json")))["libraryDiscovery"]["unitRows"]
        code = open(paths[bid], "rb").read()
        print(f"\n== {o}: falsely flagged in {len(rows)} bundles; e.g. {bid}, {len(hits)} hit prints")
        for h in hits[:2]:
            owners = sorted(det[h])
            for r in rep:
                spans = ([r["whole"]] if "whole" in r and r["whole"]["hash"] == h else []) + \
                        [f for f in r.get("functions", []) if f["hash"] == h]
                if spans:
                    s = spans[0]
                    snippet = code[s["start"]:s["start"] + 160].decode("utf-8", "replace").replace("\n", " ")
                    print(f"   print {h} (nodes {s['nodes']}, owners {owners}): {snippet}")
                    break


if __name__ == "__main__":
    main()
