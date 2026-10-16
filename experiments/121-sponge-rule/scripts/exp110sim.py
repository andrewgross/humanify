"""exp121 lab: an UPPER BOUND on exp110's index-time origin rule (crates/humanify-index/src/origin.rs, branch
exp110-index-dedup) over copylab.py rows, assuming every dependency condition holds (P depends on every co-owner,
never the reverse): P's row of a covering print is dropped when the print has <= 3 owners and another family's
smallest module is <= 4x P's + 4 KB; a seed whose remaining cover falls under half the unit's nodes is lost.
usage: <venv python> -I exp110sim.py <copy.jsonl>...   (DEV corpora only)"""
import collections
import functools
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import score  # noqa: E402
from copysim import verdict  # noqa: E402

SAME = functools.lru_cache(maxsize=None)(score.same_family)


def main():
    out = collections.Counter()
    who = collections.Counter()
    for path in sys.argv[1:]:
        for line in open(path):
            r = json.loads(line)
            if "pp" not in r:
                continue
            left = 0
            for nodes, own, others in r["pp"]:
                owners = {r["package"]} | {q for q, _ in others}
                dropped = len(owners) <= 3 and any(
                    not SAME(q, r["package"]) and m <= 4 * own + 4096 for q, m in others)
                if not dropped:
                    left += nodes
            if left < 0.5 * max(r["nodes"], 1):
                out[(r["set"], verdict(r))] += r["bytes"]
                who[(r["set"], r["package"], r["origin"] or r["truth"], verdict(r))] += r["bytes"]
    for k, v in sorted(out.items()):
        print(f"{v:>10,}  {k}")
    for k, v in who.most_common(15):
        print(f"{v:>10,}  {k}")


if __name__ == "__main__":
    main()
