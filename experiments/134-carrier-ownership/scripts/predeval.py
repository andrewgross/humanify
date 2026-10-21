"""exp134: size candidate FENCE predicates on seedfeat.py rows: per set, the wrong seed bytes a predicate removes
(gain) and the right ones (harm), and the P / F0.5 they imply against the label's summary (seed bytes only:
propagation off a fenced seed is not counted, so this is a direction, not a number). Aggregates only.

usage: python3 -I predeval.py <label> <seedfeat.jsonl> [more label=file pairs as label:file]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RESULTS = os.path.join(HERE, "../../096-library-matching-score/results")
RIGHT = ("right", "family", "origin", "selfOrigin")

PREDS = {
    "factory & ctxW>=4q+4096": lambda r: r["scope"] == "factory" and r["ctxW"] >= 4 * r["q"] + 4096,
    "factory & ctxW>=2q": lambda r: r["scope"] == "factory" and r["ctxW"] >= 2 * r["q"],
    "factory & tie": lambda r: r["scope"] == "factory" and r["fams"] > 1,
    "factory & tie & !scoped": lambda r: r["scope"] == "factory" and r["fams"] > 1 and not r["scoped"],
    "any & origin share>=.5 & ctxW>=q": lambda r: r["oN"] >= 0.5 * max(r["nW"], 1) and r["ctxW"] >= r["q"],
    "any & oN>0 & ctxW>=q & ctxW>=16oCtx": lambda r: r["oN"] > 0 and r["ctxW"] >= r["q"] and r["ctxW"] >= 16 * r["oCtx"],
    "wMod < 2% wEntry & factory": lambda r: r["scope"] == "factory" and r["wEntry"] > 50 * r["wMod"],
    "nW < nodes & factory": lambda r: r["scope"] == "factory" and r["nW"] < r["nodes"],
    "nW<.75nodes & factory": lambda r: r["scope"] == "factory" and r["nW"] < 0.75 * r["nodes"],
    "tie & stmt": lambda r: r["scope"] == "statement" and r["fams"] > 1,
}


def f05(p, r):
    return 1.25 * p * r / (0.25 * p + r) if p + r else 0.0


def main():
    pairs = [a.split(":", 1) for a in sys.argv[1:]]
    for label, path in pairs:
        summ = json.load(open(os.path.join(RESULTS, label, "summary.json")))["sets"]
        rows = [json.loads(l) for l in open(path)]
        print(f"== {label}")
        for name, pred in PREDS.items():
            g, h = collections.Counter(), collections.Counter()
            for r in rows:
                if not pred(r):
                    continue
                if r["verdict"] == "wrongFamily":
                    g[r["set"]] += r["bytes"]
                elif r["verdict"] in RIGHT:
                    h[r["set"]] += r["bytes"]
            cells = []
            for s in sorted(set(g) | set(h)):
                t = summ[s]
                named, wrong, lib = t["namedBytes"], t["wrongFamilyBytes"], t["libraryBytes"]
                p0, r0 = named / (named + wrong), named / lib
                p1, r1 = (named - h[s]) / max(named - h[s] + wrong - g[s], 1), (named - h[s]) / lib
                cells.append(f"{s} -{g[s] / 1e3:.0f}w/-{h[s] / 1e3:.0f}r dP {100 * (p1 - p0):+.2f} "
                             f"dF {100 * (f05(p1, r1) - f05(p0, r0)):+.2f}")
            print(f"  {name:40s} " + " | ".join(cells))


if __name__ == "__main__":
    main()
