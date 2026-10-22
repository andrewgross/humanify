"""exp134: which features of a carrier move (sim.py --features rows) separate the moves that FIX a wrong seed from
the ones that BREAK a right one? Prints gain (wrong->right) and harm (right->wrong) bytes per feature bucket, and
for a few candidate predicates. Aggregates only.

usage: python3 -I featsplit.py <feat.jsonl>..."""
import collections
import json
import sys

RIGHT = ("right", "family", "origin", "selfOrigin")


def cls(r):
    a = "R" if r["old"] in RIGHT else "W" if r["old"] == "wrongFamily" else "o"
    b = "R" if r["new"] in RIGHT else "W" if r["new"] == "wrongFamily" else "o"
    return {"WR": "gain", "RW": "harm"}.get(a + b)


def bucket(x, edges):
    for e in edges:
        if x < e:
            return f"<{e}"
    return f">={edges[-1]}"


FEATS = {
    "share": lambda r: bucket(r["share"], [0.1, 0.25, 0.5, 0.75, 0.9, 1.0001]),
    "ctxW/q": lambda r: bucket(r["ctxW"] / max(r["q"], 1), [0.25, 0.5, 1, 2, 4]),
    "ctxO/q": lambda r: bucket(r["ctxO"] / max(r["q"], 1), [0.1, 0.25, 0.5, 1, 2]),
    "ctxW/ctxO": lambda r: bucket(r["ctxW"] / max(r["ctxO"], 1), [8, 16, 64, 256]),
    "oSeed": lambda r: "O seeds elsewhere" if r["oSeed"] > 0 else "no O seed",
    "oUntied": lambda r: "O untied>0" if r["oUntied"] > 0 else "O untied 0",
    "wUntied": lambda r: "W untied>0" if r["wUntied"] > 0 else "W untied 0",
    "decl": lambda r: f"wDecl={r['wDecl']} oDecl={r['oDecl']}",
    "scope/k": lambda r: r["scope"] + "/" + r["k"],
    "q": lambda r: bucket(r["q"], [4096, 16384, 65536, 262144]),
}


def main():
    rows = []
    for p in sys.argv[1:]:
        rows += [json.loads(l) for l in open(p)]
    sets = sorted({r["set"] for r in rows})
    tot = collections.Counter()
    for r in rows:
        c = cls(r)
        if c:
            tot[(r["set"], c)] += r["bytes"]
    print("totals (KB):", {s: (round(tot[(s, 'gain')] / 1e3, 1), round(tot[(s, 'harm')] / 1e3, 1)) for s in sets})
    for name, f in FEATS.items():
        agg = collections.defaultdict(collections.Counter)
        for r in rows:
            c = cls(r)
            if c:
                agg[f(r)][c] += r["bytes"]
        print(f"-- {name}")
        for k in sorted(agg):
            print(f"   {k:28s} gain {agg[k]['gain'] / 1e3:9.1f}  harm {agg[k]['harm'] / 1e3:9.1f}")


if __name__ == "__main__":
    main()
