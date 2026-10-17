"""exp124 lab: of exp123's not-emitted units in a package's own pre-bundled file (truth pkg's shard was a
bundler-marker sponge), how many bytes now have an index row with the unit's exact hash under the right owner
(the truth package for own code, the origin for a recognised copy), in the re-fingerprinted lab shards.

A row existing is a ceiling for the matcher, not a claim. Points of each set's library bytes (why_dropped.py's LIB).
usage: python3 -I lab_hits.py <lab store> [<lab store> ...]    (aggregates only)
"""
import collections
import gzip
import json
import os
import sys

D = "/work/exp123-data"
CAP = f"{D}/exp122-inputs"
LIB = {"heldout": 30358072, "esbuild-app": 4916988, "wrangler": 730596, "apps-esbuild": 25643725,
       "apps-rollup": 94774012, "apps-webpack": 74482644, "pop-bun": 101194052, "pop-esbuild": 101528412}


def hashes():
    """(entry, s, e) -> unit hash, from exp122's per-unit capture."""
    out = {}
    for f in os.listdir(CAP):
        if not f.startswith("units-"):
            continue
        for line in gzip.open(f"{CAP}/{f}", "rt"):
            r = json.loads(line)
            for u in r["units"]:
                if u.get("w"):
                    out[(r["entry"], u["s"], u["e"])] = u["w"][0]
    return out


def main():
    stores = sys.argv[1:]
    aff = {l.strip() for l in open("/work/exp124-data/affected.txt")}
    h = hashes()
    ne = [json.loads(x) for x in gzip.open(f"{D}/ne-units.jsonl.gz", "rt")]
    want = collections.defaultdict(list)
    for u in ne:
        key = f"{u['slice']}/{u['pkg'].replace('/', '__')}@{u['v']}.json"
        if key in aff:
            want[key].append(u)
    for store in stores:
        tot, hit = collections.Counter(), collections.Counter()
        for key, us in want.items():
            p = f"{store}/shards/{key}"
            rows = collections.defaultdict(set)
            if os.path.exists(p):
                for r in json.load(open(p))["units"]:
                    rows[r["hash"]].add(r["owner"])
            for u in us:
                kind = "copy" if u["origin"] else "own"
                tot[u["set"], kind] += u["b"]
                hh = h.get((u["entry"], u["s"], u["e"]))
                if hh and (u["origin"] or u["pkg"]) in rows.get(hh, ()):
                    hit[u["set"], kind] += u["b"]
        print(store)
        for (st, kind), b in sorted(tot.items()):
            print(f"  {st:13s} {kind:4s} census {100 * b / LIB[st]:6.3f} pts   row now exists {100 * hit[st, kind] / LIB[st]:6.3f} pts")


if __name__ == "__main__":
    main()
