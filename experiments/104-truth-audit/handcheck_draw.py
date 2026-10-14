"""exp104: draw the hand-check sample of RELABELLED units (fixed seed, byte-weighted, 10 per rule: carried, copy,
family), plus 10 from an extra stratum `carried-claimed` (carried units a claim sat on: the byte-weighted carried
draw is all lighthouse's unclaimed locale tables, which says nothing about the claims the rule unjudges).

usage: python3 -I handcheck_draw.py <ledger rows.jsonl> <out draws.json>
Rows are ledger.py's (both step4f labels). One unit counts once per (entry, span) and rule, its claimed row kept
when a label claimed it; a rule's draws are
byte-weighted with replacement-free sampling (a drawn unit is removed and the draw repeated). The draws file holds
numbers, hashes and package names only: the verdicts are added by hand into handcheck.json."""
import hashlib
import json
import random
import sys

SEED = 104
PER_RULE = 10


def main():
    rows = [json.loads(line) for line in open(sys.argv[1])]
    pool = {}
    for r in sorted(rows, key=lambda r: r["claim"] is not None):
        rule = r["rule"].split(":")[0]
        pool.setdefault(rule, {})[(r["entry"], r["s"], r["e"])] = r
        if rule == "carried" and r["claim"] is not None:
            pool.setdefault("carried-claimed", {})[(r["entry"], r["s"], r["e"])] = r
    rng = random.Random(SEED)
    draws = []
    for rule in sorted(pool):
        units = sorted(pool[rule].values(), key=lambda r: (r["entry"], r["s"], r["e"], str(r["claim"])))
        for _ in range(min(PER_RULE, len(units))):
            total = sum(u["b"] for u in units)
            x = rng.uniform(0, total)
            acc = 0
            for k, u in enumerate(units):
                acc += u["b"]
                if acc >= x:
                    break
            u = units.pop(k)
            draws.append({"stratum": rule, "rule": u["rule"], "entry": u["entry"], "start": u["s"], "end": u["e"], "bytes": u["b"],
                          "oldLabel": u["oldLabel"], "newLabel": u["newLabel"], "origin": u["origin"],
                          "claim": u["claim"], "oldVerdict": u["old"], "newVerdict": u["new"]})
    for d in draws:
        path = f"/work/exp096-data/corpus/{d['entry']}/entry.js"
        try:
            with open(path, "rb") as fh:
                fh.seek(d["start"])
                d["unitSha256"] = hashlib.sha256(fh.read(d["end"] - d["start"])).hexdigest()[:16]
        except FileNotFoundError:
            d["unitSha256"] = None
    json.dump({"seed": SEED, "perRule": PER_RULE, "draws": draws}, open(sys.argv[2], "w"), indent=1)
    print(len(draws))


if __name__ == "__main__":
    main()
