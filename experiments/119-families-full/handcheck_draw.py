"""exp119: draw the hand-check sample of units scorer 8 RELABELLED (fixed seed 119, byte-weighted, 20 draws), dev
corpora only (dev, apps, bundlers, pop; never test / test2).

usage: python3 -I handcheck_draw.py [--seed N] <out draws.json> <ledger rows.jsonl>...
(exp120 draws with --seed 120 from its scorer-9 ledger rows, which also carry the unit's old label.)
One unit counts once per (entry, span, claim, old, new) over all labels; draws are byte-weighted without
replacement (a drawn unit is removed and the draw repeated). The verdicts are added by hand into handcheck.json."""
import json
import random
import sys

SEED = 119
N = 20


def main():
    argv = sys.argv[1:]
    seed = SEED
    if argv[:1] == ["--seed"]:
        seed, argv = int(argv[1]), argv[2:]
    out = argv[0]
    pool = {}
    for p in argv[1:]:
        for line in open(p):
            r = json.loads(line)
            if "e" not in r:
                continue  # Claude Code's sample rows carry estimated bytes, not a span
            pool.setdefault((r["entry"], r["s"], r["e"], r["claim"], r["old"], r["new"]), r)
    units = sorted(pool.values(), key=lambda r: (r["entry"], r["s"], r["e"], r["claim"]))
    rng = random.Random(seed)
    draws = []
    for _ in range(min(N, len(units))):
        x = rng.uniform(0, sum(u["b"] for u in units))
        acc = 0
        for k, u in enumerate(units):
            acc += u["b"]
            if acc >= x:
                break
        u = units.pop(k)
        draws.append({k2: u.get(k2) for k2 in ("entry", "set", "s", "e", "b", "claim", "label", "origin", "kind",
                                               "old", "new")} | ({"oldLabel": u["oldLabel"]} if "oldLabel" in u else {}))
    json.dump({"seed": seed, "n": N, "pool": len(pool), "draws": draws}, open(out, "w"), indent=1)
    print(len(draws), "of", len(pool))


if __name__ == "__main__":
    main()
