"""exp123 step 1a: exp122's `not-emitted` class-d units, one row each, with the truth file and the build dirs.

Re-derives exp122's census cause per unit (scripts/census.py on branch exp122-normaliser: no-build, then any
normalisation recovering it, then not-emitted = no counterpart or token-diff ratio < 0.5) from exp122's PRIVATE
per-unit files, and keeps the not-emitted ones.

usage: python3 -I select_ne.py          -> /work/exp123-data/ne-units.jsonl.gz (PRIVATE: per unit, outside the repo)
Prints aggregates only.
"""
import collections
import gzip
import json
import os

D = "/work/exp123-data/exp122-inputs"  # exp122's private per-unit files, copied before /work/exp122-data was pruned
OUT = "/work/exp123-data/ne-units.jsonl.gz"
VARIANTS = list("KTCPMDALIO") + ["KTCPMDALIO"]


def recovered():
    out = set()
    for v in VARIANTS:
        p = f"{D}/recovered-{v}.jsonl.gz"
        if os.path.exists(p):
            out |= {tuple(x) for x in map(json.loads, gzip.open(p, "rt"))}
    return out


def main():
    rec = recovered()
    du = {(u["entry"], u["s"], u["e"]): u for u in map(json.loads, gzip.open(f"{D}/plan/dunits.jsonl.gz", "rt"))}
    stat = collections.Counter()
    out = gzip.open(OUT, "wt")
    for line in gzip.open(f"{D}/diffs.jsonl.gz", "rt"):
        r = json.loads(line)
        k = (r["entry"], r["s"], r["e"])
        if r["status"] == "noBuild" or k in rec:
            continue
        if not (r["status"] in ("noQuery", "noCounterpart") or r.get("ratio", 0) < 0.5):
            continue
        u = du[k]
        stat[r["set"]] += u["b"]
        out.write(json.dumps({"set": r["set"], "entry": r["entry"], "slice": u["slice"], "pkg": u["pkg"], "v": u["v"],
                              "file": u["file"], "s": u["s"], "e": u["e"], "origin": u.get("origin"), "b": u["b"], "cls": u["cls"], "status": r["status"],
                              "ratio": r.get("ratio"), "dirs": u["dirs"]}) + "\n")
    out.close()
    print({k: v for k, v in sorted(stat.items())})


if __name__ == "__main__":
    main()
