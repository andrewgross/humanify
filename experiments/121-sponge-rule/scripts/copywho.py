"""exp121 lab: which (claimer, origin Q, truth) groups the package-level copy rule removes, by verdict bytes.
usage: <venv python> -I copywho.py F theta <copy.jsonl>... (DEV corpora only)"""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from copysim import origin_share, verdict  # noqa: E402


def main():
    F, theta = float(sys.argv[1]), float(sys.argv[2])
    rows = []
    for p in sys.argv[3:]:
        rows += [json.loads(l) for l in open(p)]
    seeds = [r for r in rows if "co" in r]
    allc = collections.defaultdict(lambda: True)
    info = {}
    for r in seeds:
        s, q = origin_share(r, F)
        k = (r["entry"], r["package"])
        allc[k] = allc[k] and s >= theta
        info.setdefault(k, collections.Counter())[q] += r["bytes"]
    out = collections.Counter()
    for r in seeds:
        k = (r["entry"], r["package"])
        if allc[k]:
            q = info[k].most_common(1)[0][0]
            out[(r["set"], r["package"], q, r["origin"] or r["truth"], verdict(r))] += r["bytes"]
    for k, v in out.most_common(40):
        print(f"{v:>10,}  {k}")


if __name__ == "__main__":
    main()
