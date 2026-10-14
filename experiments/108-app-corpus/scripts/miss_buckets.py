"""exp108: exp101's aggregate miss buckets, with "training closure" = the ENLARGED training corpus
(exp095's 293 training apps + exp108's new apps). Prints and writes aggregates only, never a held-out package name.

usage: <exp095 venv python> -I miss_buckets.py <label> <out.json>
"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.normpath(os.path.join(HERE, "../../101-index-coverage/scripts")))
import miss_buckets as mb  # noqa: E402

SEL108 = os.environ.get("SEL108", "/work/exp108-data")
_base = mb.training_closure


def enlarged():
    pk, napps = _base()
    for slug in sorted(os.listdir(f"{SEL108}/apps")):
        p = f"{SEL108}/apps/{slug}/closure.json"
        if not os.path.exists(p):
            continue
        c = json.load(open(p))
        if not c.get("packages"):
            continue
        napps += 1
        for name in c["packages"]:
            pk[name] += 1
    return collections.Counter(pk), napps


mb.training_closure = enlarged

if __name__ == "__main__":
    mb.main()
