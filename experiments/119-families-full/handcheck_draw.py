"""exp119: draw the hand-check sample of units scorer 8 RELABELLED (fixed seed 119, byte-weighted, 20 draws), dev
corpora only (dev, apps, bundlers, pop; never test / test2).

usage: python3 -I handcheck_draw.py <out draws.json> <ledger rows.jsonl>...
One unit counts once per (entry, span, claim, old, new) over all labels; draws are byte-weighted without
replacement (a drawn unit is removed and the draw repeated). The verdicts are added by hand into handcheck.json."""
import json
import random
import sys

SEED = 119
N = 20


def main():
    out = sys.argv[1]
    pool = {}
    for p in sys.argv[2:]:
        for line in open(p):
            r = json.loads(line)
            if "e" not in r:
                continue  # Claude Code's sample rows carry estimated bytes, not a span
            pool.setdefault((r["entry"], r["s"], r["e"], r["claim"], r["old"], r["new"]), r)
    units = sorted(pool.values(), key=lambda r: (r["entry"], r["s"], r["e"], r["claim"]))
    rng = random.Random(SEED)
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
                                               "old", "new")})
    json.dump({"seed": SEED, "n": N, "pool": len(pool), "draws": draws}, open(out, "w"), indent=1)
    print(len(draws), "of", len(pool))


if __name__ == "__main__":
    main()
