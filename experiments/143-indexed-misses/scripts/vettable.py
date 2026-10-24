"""exp143: vetsim.py rows -> per set KB of vetoed seed bytes the claim would get RIGHT vs WRONG (library units of
another family, app, self) by exemption feature. usage: python3 -I vettable.py <vetsim.jsonl> <out.json>"""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ceiling  # noqa: E402

FEATURES = {
    "all vetoed": lambda r: True,
    "unambiguous": lambda r: not r.get("ambiguous"),
    "unambiguous, exact whole print": lambda r: not r.get("ambiguous") and r.get("exact"),
    "unambiguous, factory body": lambda r: not r.get("ambiguous") and r["kind"] == "FactoryBody",
    "unambiguous, >= half its module": lambda r: not r.get("ambiguous") and (r["modShare"] or 0) >= 0.5,
    "unambiguous, exact, >= half its module": lambda r: not r.get("ambiguous") and r.get("exact")
    and (r["modShare"] or 0) >= 0.5,
    "unambiguous, exact, >= 1 KB": lambda r: not r.get("ambiguous") and r.get("exact") and r["bytes"] >= 1024,
}


def cell(c):
    if not c:
        return "-"
    return f"{c.get('right', 0):,.0f} / {c.get('wrong', 0):,.1f} ({c['sim']['dP']:+.3f} / {c['sim']['dF05']:+.2f})"


def main():
    rows = [json.loads(line) for line in open(sys.argv[1])]
    out = collections.defaultdict(dict)
    for name, f in FEATURES.items():
        acc = collections.defaultdict(collections.Counter)
        for r in rows:
            if f(r) and not r["baseClaimed"]:
                acc[r["set"]]["right" if r["right"] else "wrong" if r["verdict"] in ("wrongFamily", "app")
                              else "unscored"] += r["bytes"]
        for s, c in acc.items():
            out[s][name] = {k: round(v / 1024, 1) for k, v in c.items()}
    base = ceiling.base_sets()
    for s, d in out.items():
        for name, c in d.items():
            (p0, r0, f0), (p1, r1, f1) = ceiling.sim(base[s], c.get("right", 0) * 1024, c.get("wrong", 0) * 1024)
            c["sim"] = {"dP": round(p1 - p0, 3), "dR": round(r1 - r0, 2), "dF05": round(f1 - f0, 3)}
    json.dump(out, open(sys.argv[2], "w"), indent=1, sort_keys=True)
    print("(right KB / wrong KB, simulated d P / d F0.5)")
    names = list(FEATURES)
    print("| set | " + " | ".join(f"{n} right / wrong KB" for n in names) + " |")
    for s in sorted(out):
        print(f"| {s} | " + " | ".join(cell(out[s].get(n))
                                       for n in names) + " |")


if __name__ == "__main__":
    main()
