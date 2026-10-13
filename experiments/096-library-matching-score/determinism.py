"""exp096: are two runs of the same configuration byte-identical? Compares two labels' raw discovery reports
(<DATA>/results/<label>/reports/*.json, which carry no label name) and their cards and summaries (the `label` key
aside). Both labels must share binary, index, rule and corpus. On success writes bands.json: the score's noise band
is ZERO, so any movement between labels is real.

usage: <venv python> -I determinism.py <label> <repeat label>"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib096  # noqa: E402
from lib096 import DATA, HERE, dump, sha256_file  # noqa: E402

SAME = ("buildCommit", "binarySha256", "indexId", "rule", "corpusSha256")


def strip_label(path):
    d = json.load(open(path))
    d.pop("label", None)
    return dump(d)


def main():
    a, b = sys.argv[1:3]
    pa, pb = (json.load(open(os.path.join(DATA, "results", x, "pipeline.json"))) for x in (a, b))
    diff = [k for k in SAME if pa[k] != pb[k]]
    if diff:
        raise SystemExit(f"REFUSED: the labels differ in {diff} — that measures a change, not a floor")
    bad, n = [], 0
    for e in lib096.load_manifest()["entries"]:
        ra, rb = (os.path.join(DATA, "results", x, "reports", e["id"] + ".json") for x in (a, b))
        n += 1
        if sha256_file(ra) != sha256_file(rb):
            bad.append(("report", e["id"]))
        ca, cb = (os.path.join(HERE, "results", x, "cards", e["id"] + ".json") for x in (a, b))
        if strip_label(ca) != strip_label(cb):
            bad.append(("card", e["id"]))
    if strip_label(os.path.join(HERE, "results", a, "summary.json")) != strip_label(
            os.path.join(HERE, "results", b, "summary.json")):
        bad.append(("summary", "-"))
    if bad:
        print(f"NOT DETERMINISTIC: {len(bad)} difference(s): {bad[:10]}")
        sys.exit(1)
    print(f"DETERMINISTIC: {n} reports, {n} cards and the summary byte-identical ({a} vs {b})")
    open(os.path.join(HERE, "bands.json"), "w").write(dump({
        "provenance": {"labels": [a, b], "buildCommit": pa["buildCommit"], "binarySha256": pa["binarySha256"],
                       "indexId": pa["indexId"], "rule": pa["rule"], "corpusSha256": pa["corpusSha256"],
                       "method": "two runs of one configuration: every raw report and every card byte-identical"},
        "band": 0,
        "meaning": "library discovery is deterministic (no LLM, no sampling): the noise band of every card "
                   "column is ZERO, so any movement between labels is a real effect of the change"}))


if __name__ == "__main__":
    main()
