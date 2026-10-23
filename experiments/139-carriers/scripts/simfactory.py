"""exp139: factory-majority simulation on terser-gained seeds (carriers.py dossier + judge13.py rows). A cut
CommonJS factory is ONE file of one package: a terser-gained index seed of W on a body statement of factory F,
where another family Q holds >= K x W's index-seed bytes in F, is refused (or, --give, goes to Q when Q scores it).
Prints moved bytes per set by verdict change. Aggregates only.

usage: <venv python> -I simfactory.py <judge.jsonl> <slot> <dossier.jsonl> [--k 4] [--all]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
RIGHT = ("right", "family", "origin", "selfOrigin")


def main():
    import score
    same = score.same_family
    judge, slot, dossier = sys.argv[1:4]
    k = float(sys.argv[sys.argv.index("--k") + 1]) if "--k" in sys.argv else 4.0
    every = "--all" in sys.argv
    mod = {}
    for line in open(judge):
        r = json.loads(line)
        if r.get(slot) and r.get("module") is not None:
            mod[(r["entry"], r["start"], r["end"])] = r["module"]
    xs = [json.loads(line) for line in open(dossier)]
    per = collections.defaultdict(collections.Counter)
    for x in xs:
        m = mod.get((x["entry"], x["start"], x["end"]))
        if m is not None:
            per[(x["entry"], m)][x["package"]] += x["bytes"]
    out = collections.Counter()
    for x in xs:
        w, sc = x["package"], x["scores"]
        s0 = x.get("scores0", sc)
        wn = sc.get(w, [0])[0]
        gained = x.get("slices", 1) > 1 and (w not in s0 or s0[w][0] < wn)
        if not (gained or every):
            continue
        m = mod.get((x["entry"], x["start"], x["end"]))
        if m is None:
            continue
        fam = collections.Counter()
        for p, b in per[(x["entry"], m)].items():
            key = next((q for q in fam if same(q, p)), p)
            fam[key] += b
        mine = sum(b for p, b in fam.items() if same(p, w))
        rivals = [(b, p) for p, b in fam.items() if not same(p, w) and b >= k * mine]
        if not rivals:
            continue
        a = "right" if x["verdict"] in RIGHT else "wrong" if x["verdict"] == "wrongFamily" else "other"
        out[(x["set"], f"{a}->refused")] += x["bytes"]
    for key, b in sorted(out.items()):
        print(f"{b / 1e3:9.1f} KB  " + "  ".join(key))


if __name__ == "__main__":
    main()
