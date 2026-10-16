"""exp121 lab: package-level copy rule where a package counts as PRESENT only when its non-copy index seeds hold at
least `frac` of its seed bytes in the bundle (frac 0 = exp121's shipped rule: any one non-copy seed).
usage: <venv python> -I copyfrac.py F theta <copy.jsonl>...  (DEV corpora only)"""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from copysim import origin_share, verdict  # noqa: E402


def main():
    F, theta = float(sys.argv[1]), float(sys.argv[2])
    seeds = []
    for p in sys.argv[3:]:
        seeds += [r for r in map(json.loads, open(p)) if "co" in r]
    tot, own = collections.Counter(), collections.Counter()
    for r in seeds:
        k = (r["entry"], r["package"])
        tot[k] += r["bytes"]
        if origin_share(r, F)[0] < theta:
            own[k] += r["bytes"]
    for frac in (0.0, 0.05, 0.1, 0.25, 0.5):
        out = collections.Counter()
        for r in seeds:
            k = (r["entry"], r["package"])
            if (own[k] == 0 or own[k] < frac * tot[k]) and origin_share(r, F)[0] >= theta:
                out[(r["set"], verdict(r))] += r["bytes"]
        sets = sorted({s for s, _ in out})
        print(f"frac {frac}: " + "  ".join(f"{s} -good {out[(s, 'good')]:,} -wrong {out[(s, 'wrong')]:,}" for s in sets))


if __name__ == "__main__":
    main()
