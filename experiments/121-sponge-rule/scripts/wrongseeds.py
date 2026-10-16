"""exp121 lab: wrong-family bytes of a seedlab.py file (exp114) by claimer and by where the truth / origin stands in
the seed's index evidence. DEV corpora only. usage: <venv python> -I wrongseeds.py <seeds.jsonl> [top]"""
import collections
import functools
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import score  # noqa: E402

SAME = functools.lru_cache(maxsize=None)(score.same_family)


def where(r):
    t = [x for x in (r["truth"], r["origin"]) if x and not x.startswith("<")]
    if not t:
        return "truth-not-pkg"
    if any(SAME(a, x) for a in r.get("accept", []) for x in t):
        return "truth-accepted"
    if any(SAME(a, x) for a in r.get("holders", {}) for x in t):
        return "truth-holds-print"
    return "truth-absent"


def main():
    rows = [json.loads(l) for l in open(sys.argv[1])]
    top = int(sys.argv[2]) if len(sys.argv) > 2 else 25
    by_rule = collections.Counter()
    by_where = collections.Counter()
    by_claimer = collections.Counter()
    for r in rows:
        if r["verdict"] != "wrongFamily":
            continue
        by_rule[(r["set"], r["rule"], r["evidence"])] += r["bytes"]
        if "accept" in r:
            by_where[(r["set"], where(r), "origin" if r["origin"] else "shipper")] += r["bytes"]
            by_claimer[(r["set"], r["package"], r["origin"] or r["truth"], where(r))] += r["bytes"]
    for name, c in (("rule", by_rule), ("where", by_where)):
        print("==", name)
        for k, v in sorted(c.items()):
            print(f"{v:>10,}  {k}")
    print("== claimer")
    for k, v in c.most_common(0) or by_claimer.most_common(top):
        print(f"{v:>10,}  {k}")


if __name__ == "__main__":
    main()
