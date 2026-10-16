"""exp121 lab: seed-level simulation of copy/origin rules over copylab.py rows (no propagation: a design estimate;
the Rust run is the measurement). DEV corpora only.

A seed of P is a COPY seed when some other family Q holds >= theta of P's covered nodes in index modules at most
1/F of P's module for the same print (Q is the smaller context: the origin P inlined).
  seed     refuse each copy seed (named by no one)
  pkg      refuse P in the bundle only when EVERY index seed of P there is a copy seed
usage: <venv python> -I copysim.py <copy.jsonl>... """
import collections
import functools
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import score  # noqa: E402

SAME = functools.lru_cache(maxsize=None)(score.same_family)
GOOD = ("right", "family", "origin")


def origin_share(r, F, slack=0, minq=0):
    """max over other families Q of (nodes Q co-holds at <= P's module / F) / P's covered nodes; and that Q."""
    if not r.get("cov"):
        return 0.0, None
    best, bq = 0.0, None
    for q, v in r["co"].items():
        if SAME(q, r["package"]):
            continue
        n = sum(x[0] for x in v if x[2] * F + slack <= x[1] and x[2] >= minq)
        s = n / r["cov"]
        if s > best:
            best, bq = s, q
    return best, bq


def verdict(r):
    return "good" if r["verdict"] in GOOD else ("wrong" if r["verdict"] == "wrongFamily" else "other")


def run(rows, theta, F, mode, slack=0):
    out = collections.Counter()
    seeds = [r for r in rows if "co" in r]
    flag = {}
    for i, r in enumerate(seeds):
        s, q = origin_share(r, F, slack)
        flag[i] = s >= theta
    if mode == "pkg":
        allc = collections.defaultdict(lambda: True)
        for i, r in enumerate(seeds):
            k = (r["entry"], r["package"])
            allc[k] = allc[k] and flag[i]
        flag = {i: allc[(r["entry"], r["package"])] for i, r in enumerate(seeds)}
    for i, r in enumerate(seeds):
        if flag[i]:
            out[(r["set"], verdict(r))] += r["bytes"]
    return out


def main():
    rows = []
    for p in sys.argv[1:]:
        rows += [json.loads(l) for l in open(p)]
    base = collections.Counter()
    for r in rows:
        if "co" in r:
            base[(r["set"], verdict(r))] += r["bytes"]
    sets = sorted({k[0] for k in base})
    print("seed bytes:", {s: (base[(s, "good")], base[(s, "wrong")]) for s in sets})
    for mode in ("seed", "pkg"):
        for F in (4, 16, 64):
            for theta in (0.05, 0.1, 0.2, 0.3, 0.5):
                o = run(rows, theta, F, mode)
                print(f"{mode:4} F={F:<3} theta={theta:<4} " + "  ".join(
                    f"{s}: -good {o[(s, 'good')]:>9,} -wrong {o[(s, 'wrong')]:>9,}" for s in sets))


if __name__ == "__main__":
    main()
