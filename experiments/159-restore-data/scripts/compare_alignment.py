#!/usr/bin/env python3
"""exp159 census A: how much of a minified slice's unit rows does the SAME
recipe's unminified build align with, by unit print hash?

The alignment anchor is the canonical serialize's slot ordinals: two
structurally identical subtrees hash equal AND number their local slots
alike (first occurrence in the fixed alphabetical walk), so a minified
unit whose print hash has an equal-hash unminified counterpart can carry
that counterpart's declared names, in walk-ordinal order, as the
restoration answer for its slots.

Reads the two slices' SHARDS (rows >= the 20-node row floor, owner
attributed) and reports, per package and aggregate:
  aligned      distinct (owner,file,hash) rows in both slices
  minified     distinct rows in the minified slice
  nodes share  node-weighted aligned share of the minified rows
  fileFree     alignment on (owner,hash) — file-agnostic (attribution
               differences between the two builds' sourcemaps)

Usage: compare_alignment.py <store> <min-slice> <plain-slice> <out.json>
"""
import glob
import json
import os
import sys
from collections import defaultdict


def rows(slice_dir: str) -> list[dict]:
    out = []
    for path in sorted(glob.glob(os.path.join(slice_dir, "*.json"))):
        shard = json.load(open(path))
        for u in shard.get("units", []):
            out.append(u)
    return out


def main() -> None:
    store, min_slice, plain_slice, out_path = sys.argv[1:5]
    m = rows(os.path.join(store, "shards", min_slice))
    p = rows(os.path.join(store, "shards", plain_slice))
    pkgs = defaultdict(lambda: {
        "min": set(), "minNodes": 0, "plain": set(),
        "aligned": set(), "alignedNodes": 0,
        "minOwnerHash": set(), "plainOwnerHash": set(),
        "alignedOwnerHash": set(),
    })
    for u in m:
        e = pkgs[u["owner"]]
        k = (u["owner"], u["file"], u["hash"])
        if k not in e["min"]:
            e["min"].add(k)
            e["minNodes"] += u["nodes"]
        e["minOwnerHash"].add((u["owner"], u["hash"]))
    for u in p:
        e = pkgs[u["owner"]]
        k = (u["owner"], u["file"], u["hash"])
        e["plain"].add(k)
        e["plainOwnerHash"].add((u["owner"], u["hash"]))
        if k in e["min"]:
            e["aligned"].add(k)
            e["alignedNodes"] += u["nodes"]
        if (u["owner"], u["hash"]) in e["minOwnerHash"]:
            e["alignedOwnerHash"].add((u["owner"], u["hash"]))
    per_package = {}
    tot = defaultdict(int)
    for owner, e in pkgs.items():
        n = len(e["min"])
        per_package[owner] = {
            "minified": n,
            "plain": len(e["plain"]),
            "aligned": len(e["aligned"]),
            "alignedShare": round(len(e["aligned"]) / n, 4) if n else None,
            "nodeShare": round(e["alignedNodes"] / e["minNodes"], 4) if e["minNodes"] else None,
            "fileFreeShare": round(len(e["alignedOwnerHash"]) / len(e["minOwnerHash"]), 4)
            if e["minOwnerHash"] else None,
        }
        for k in ("min", "plain", "aligned"):
            tot[k] += len(e[k])
        tot["minNodes"] += e["minNodes"]
        tot["alignedNodes"] += e["alignedNodes"]
        tot["minOwnerHash"] += len(e["minOwnerHash"])
        tot["alignedOwnerHash"] += len(e["alignedOwnerHash"])
    result = {
        "slices": {"minified": min_slice, "plain": plain_slice},
        "packages": len(per_package),
        "aggregate": {
            "minifiedRows": tot["min"],
            "plainRows": tot["plain"],
            "alignedRows": tot["aligned"],
            "alignedShare": round(tot["aligned"] / tot["min"], 4) if tot["min"] else None,
            "nodeShare": round(tot["alignedNodes"] / tot["minNodes"], 4) if tot["minNodes"] else None,
            "fileFreeShare": round(tot["alignedOwnerHash"] / tot["minOwnerHash"], 4)
            if tot["minOwnerHash"] else None,
        },
        "perPackage": per_package,
    }
    json.dump(result, open(out_path, "w"), indent=1, sort_keys=True)
    print(json.dumps(result["aggregate"], indent=1))
    print("packages:", result["packages"])


if __name__ == "__main__":
    main()
