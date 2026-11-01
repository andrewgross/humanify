#!/usr/bin/env python3
"""exp180 step 2: one compact, truth-labelled extract per label, so every detector simulation is pure Python.

usage: <exp095 venv python> -I extract.py <corpus> <label> <out.json.gz>

Reads a scored label's `humanify discover --unit-prints` reports and labels each unit EXACTLY as scorer 14 does
(exp096 score.py: score.label_units with every frozen truth layer for source-map corpora; the labelled sample and
span-containment claims for Claude Code). Writes, per bundle:

  prints   every print in the bundle (whole + function hashes of EVERY unit row, scored or not): what a detector
           lookup would see
  units    per scored unit [bytes, label, version, origin, via, claimed package or null, verdict, [its prints]]
           (verdict = score.judge_origin's: right/family/origin count as named, as in the scorer)

Nothing from the CC truth is committed: the extract lives under /work/exp180-data only.
"""
import bisect
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.normpath(os.path.join(HERE, "../096-library-matching-score")))
import lib096  # noqa: E402

CORPUS, LABEL, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
lib096.use(CORPUS)
import score  # noqa: E402
import truth_audit  # noqa: E402


def unit_prints(u):
    if "w" not in u:
        return []
    return [u["w"][0]] + [f[2] for f in u["f"]]


def map_bundle(entry, report, truth, origins, audit):
    units = score.run_units(report)
    labels = score.label_units(truth, units, origins, audit)
    app = truth["appPackage"]
    every = score.scored_units(units)
    key = {(u["s"], u["e"]): u["i"] for u in every}
    claims = {key[(c["start"], c["end"])]: c["package"] for c in report.get("claims", [])
              if (c["start"], c["end"]) in key}
    rows = []
    for u in every:
        t = labels[u["i"]]
        c = claims.get(u["i"])
        v = score.judge_origin(c, t, app)[0] if c is not None else None
        rows.append([u["b"], t["label"], t.get("version"), t.get("origin"), t.get("via"), c, v, unit_prints(u)])
    return {"appPackage": app, "units": rows, "prints": sorted({h for u in units for h in unit_prints(u)})}


def sample_bundle(entry, report, truth):
    units = score.run_units(report)
    by_s = {u["s"]: u for u in score.statement_units(units)}
    strata = truth["strata"]
    sb = {}
    for t in truth["rows"]:
        sb[t["stratum"]] = sb.get(t["stratum"], 0) + t["b"]
    cl = sorted((c["start"], c["end"], c["package"]) for c in report.get("claims", []))
    starts = [c[0] for c in cl]

    def claim_at(s):
        k = bisect.bisect_right(starts, s) - 1
        return cl[k][2] if k >= 0 and cl[k][0] <= s < cl[k][1] else None
    rows = []
    for t in truth["rows"]:
        est = strata[t["stratum"]]["bytes"] * t["b"] / sb[t["stratum"]]
        p = claim_at(t["s"])
        lib = "pkg" in t
        lab = t["pkg"] if lib else t["label"]
        v = None
        if p is not None:
            v = ("right" if p == t["pkg"] else "family" if score.same_family(p, t["pkg"]) else "wrongFamily") \
                if lib else {"<app>": "app"}.get(t["label"], t["label"].strip("<>"))
        u = by_s.get(t["s"])
        rows.append([est, lab, t.get("version"), None, None, p, v, unit_prints(u) if u else []])
    return {"appPackage": None, "units": rows, "prints": sorted({h for u in units for h in unit_prints(u)}),
            "estimate": "stratified sample: unit bytes are src-wide estimates"}


def main():
    run_dir = os.path.join(lib096.DATA, "results", LABEL)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    manifest = lib096.load_manifest()
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    out = {"corpus": CORPUS, "label": LABEL, "indexId": pipeline["indexId"], "bundles": []}
    for entry in manifest["entries"]:
        rp = os.path.join(run_dir, "reports", entry["id"] + ".json")
        doc = json.load(open(rp))
        report = doc["libraryDiscovery"]
        if report is None:
            continue
        if report.get("indexId") != pipeline["indexId"]:
            raise SystemExit(f"REFUSED: {entry['id']} ran on another index")
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m) if entry["truth"]["method"] == "sourcemap" else None
        truth, origins = truth_audit.redecode(entry, truth, origins)
        if truth["method"] == "sample":
            b = sample_bundle(entry, report, truth)
        else:
            b = map_bundle(entry, report, truth, origins, truth_audit.Audit(entry, truth, cm))
        b.update(id=entry["id"], set=entry["set"], slice=doc["toolchain"]["slice"])
        out["bundles"].append(b)
        print(entry["id"], len(b["units"]), len(b["prints"]), flush=True)
    with gzip.open(OUT, "wt") as fh:
        json.dump(out, fh)


if __name__ == "__main__":
    main()
