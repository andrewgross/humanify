"""exp145: how often would a 36-bundle terser judge set fail by ONE lumpy event, given the candidate's DEV events?

usage: python3 events145.py <bootstrap.json from all145.py>
tolerance T = (1 - P_base) x (the candidate's new named bytes per 36 terser apps, pooled dev terser): the new wrong
bytes a 36-app set can absorb before its precision drops. q = share of dev bundles (every dev label pair of the
candidate, all sets) whose new wrong >= T. P(fail) ~ 1 - (1 - q)^36, with q's rule-of-three upper bound when no
bundle reaches T."""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from all145 import CANDS, WIDE  # noqa: E402
from pairs145 import load  # noqa: E402

# dev label sets with identical cards to main for exp140 / exp141 (exp141 README: 285/285 identical off remin*)
ZERO_BUNDLES = 285


def adds(name):
    out = []
    pairs = [(r, b, c) for r, b, c in CANDS[name][2].values()] + WIDE.get(name, [])
    for results, base, cand in pairs:
        b, c = load(results, base), load(results, cand)
        for s in b:
            out += [c[s][a][1] - b[s][a][1] for a in b[s]]
    if name not in WIDE:
        out += [0] * ZERO_BUNDLES
    return out


def main(path):
    res = json.load(open(path))
    print(f"{'candidate':<36}{'T KB':>7}{'bundles':>9}{'>=T':>5}{'q':>8}{'P(fail36)':>11}{'(3/n bound)':>12}  judge")
    for name, e in res.items():
        pt = e["pooled-terser"]
        t = (1 - pt["P_base"]) * pt["dNamedKB"] * 1e3 / pt["apps"] * 36
        a = adds(name)
        hit = sum(1 for x in a if x >= t)
        q = hit / len(a)
        pf = 1 - (1 - q) ** 36
        ub = 1 - (1 - 3 / len(a)) ** 36
        print(f"{name:<36}{t/1e3:>7.2f}{len(a):>9}{hit:>5}{q:>8.4f}{pf:>11.3f}{ub:>12.3f}  {e['judge']}")
        e["eventStress"] = {"toleranceKB": t / 1e3, "bundles": len(a), "atOrAboveT": hit, "q": q, "pFail36": pf,
                            "pFail36RuleOfThree": ub}
    json.dump(res, open(path, "w"), indent=1, sort_keys=True)


if __name__ == "__main__":
    main(sys.argv[1])
