"""exp134: feature distributions of WRONG vs RIGHT index seeds (seedfeat.py rows), per scope, bytes-weighted: where a
fence could separate them. Aggregates only.

usage: python3 -I featdist.py <seedfeat.jsonl>... [--scope factory|statement]"""
import collections
import json
import sys

RIGHT = ("right", "family", "origin", "selfOrigin")


def bucket(x, edges):
    for e in edges:
        if x < e:
            return f"<{e}"
    return f">={edges[-1]}"


FEATS = {
    "known": lambda r: str(r["known"]),
    "fams": lambda r: str(min(r["fams"], 3)),
    "nW/nodes": lambda r: bucket(r["nW"] / max(r["nodes"], 1), [0.6, 0.8, 0.95, 1.0001]),
    "ctxW/q": lambda r: bucket(r["ctxW"] / max(r["q"], 1), [0.5, 1, 1.5, 2, 4]),
    "wMod/wEntry": lambda r: bucket(r["wMod"] / max(r["wEntry"], 1), [0.01, 0.05, 0.2, 0.5, 1.0001]),
    "wEntry KB": lambda r: bucket(r["wEntry"] / 1e3, [1, 5, 20, 100, 1000]),
    "n2/nW": lambda r: bucket(r["n2"] / max(r["nW"], 1), [0.01, 0.5, 0.9, 1.0001]),
    "oN>0": lambda r: str(r["oN"] > 0),
    "scoped": lambda r: str(r["scoped"]),
    "q KB": lambda r: bucket(r["q"] / 1e3, [1, 4, 16, 64, 256]),
}


def main():
    paths = [a for a in sys.argv[1:] if not a.startswith("--") and a not in ("factory", "statement")]
    scope = sys.argv[sys.argv.index("--scope") + 1] if "--scope" in sys.argv else None
    rows = [json.loads(l) for p in paths for l in open(p)]
    rows = [r for r in rows if scope is None or r["scope"] == scope]
    w = sum(r["bytes"] for r in rows if r["verdict"] == "wrongFamily")
    ri = sum(r["bytes"] for r in rows if r["verdict"] in RIGHT)
    print(f"wrong {w / 1e3:.1f} KB, right {ri / 1e3:.1f} KB")
    for name, f in FEATS.items():
        agg = collections.defaultdict(collections.Counter)
        for r in rows:
            v = "wrong" if r["verdict"] == "wrongFamily" else "right" if r["verdict"] in RIGHT else None
            if v:
                agg[f(r)][v] += r["bytes"]
        print(f"-- {name}")
        for k in sorted(agg):
            a = agg[k]
            print(f"   {k:10s} wrong {a['wrong'] / 1e3:9.1f}  right {a['right'] / 1e3:11.1f}  "
                  f"wrong share {a['wrong'] / max(a['wrong'] + a['right'], 1):.3%}")


if __name__ == "__main__":
    main()
