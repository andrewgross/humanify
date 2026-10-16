"""exp121 lab: for the seeds the package-level copy rule drops (copysim.py 'pkg'), how often the best co-holding
family Q (the origin candidate) is the truth or its origin: the 'prefer the origin' variant's ceiling.
usage: <venv python> -I reassign.py F theta <copy.jsonl>...  (DEV corpora only)"""
import collections
import functools
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import score  # noqa: E402
from copysim import origin_share, verdict  # noqa: E402

SAME = functools.lru_cache(maxsize=None)(score.same_family)


def main():
    F, theta = float(sys.argv[1]), float(sys.argv[2])
    seeds = []
    for p in sys.argv[3:]:
        seeds += [r for r in map(json.loads, open(p)) if "co" in r]
    allc = collections.defaultdict(lambda: True)
    for r in seeds:
        allc[(r["entry"], r["package"])] &= origin_share(r, F)[0] >= theta
    out = collections.Counter()
    for r in seeds:
        if not allc[(r["entry"], r["package"])]:
            continue
        _, q = origin_share(r, F)
        ok = q and (SAME(q, r["truth"]) or (r["origin"] and SAME(q, r["origin"])))
        out[(r["set"], verdict(r), "Q-right" if ok else "Q-wrong")] += r["bytes"]
    for k, v in sorted(out.items()):
        print(f"{v:>10,}  {k}")


if __name__ == "__main__":
    main()
