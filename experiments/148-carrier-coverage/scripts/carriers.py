"""exp148 step 3: which packages of a CANDIDATE index (a superset build, e.g. exp133's download top-25k) are
CARRIERS of code the base index already holds — corpus-blind, read only from the two indexes' tables.

A unit row of a candidate-only package P is `held` when some package of the BASE index owns the same whole print
in the candidate index's table (same slice). Per (P, file): units, held units, held nodes, module bytes, and the base
owners of the held prints. A COPIED FILE is one with >= MIN_UNITS units of >= MIN_NODES nodes, all held, module
>= MIN_BYTES; a CARRIER is a package with >= 1 copied file. Rows are hash-sorted, so one streaming pass groups every
owner of a print together.

usage: python3 -I carriers.py <candidate ix> <base ix> <slice> <out.json>"""
import collections
import json
import os
import sys

MIN_NODES, MIN_UNITS, MIN_BYTES = 20, 3, 1024


def main():
    cand, base, slice_, out = sys.argv[1:5]
    base_pk = set(json.load(open(os.path.join(base, "versions.json"))))
    files = collections.defaultdict(lambda: [0, 0, 0, 0, 0, collections.Counter()])  # units held nodes heldN bytes owners
    cur, group = None, []

    def flush():
        owners = {r["owner"] for r in group}
        based = sorted(o for o in owners if o in base_pk)
        for r in group:
            if r["owner"] in base_pk or r["nodes"] < MIN_NODES:
                continue
            f = files[(r["owner"], r["file"])]
            f[0] += 1
            f[2] += r["nodes"]
            f[4] = max(f[4], r.get("moduleBytes") or 0)
            if based:
                f[1] += 1
                f[3] += r["nodes"]
                for o in based:
                    f[5][o] += 1

    with open(os.path.join(cand, f"units.{slice_}.jsonl")) as fh:
        for line in fh:
            h = line[9:25]
            if h != cur:
                if group:
                    flush()
                cur, group = h, []
            r = json.loads(line)
            group.append({"owner": r["owner"], "file": r["file"], "nodes": r["nodes"], "moduleBytes": r.get("moduleBytes")})
    if group:
        flush()
    pk = collections.defaultdict(lambda: {"files": 0, "units": 0, "held": 0, "copiedFiles": 0, "copiedBytes": 0,
                                          "bytes": 0, "origins": collections.Counter()})
    for (p, f), (u, hu, n, hn, b, own) in files.items():
        d = pk[p]
        d["files"] += 1
        d["units"] += u
        d["held"] += hu
        d["bytes"] += b
        if u >= MIN_UNITS and hu == u and b >= MIN_BYTES:
            d["copiedFiles"] += 1
            d["copiedBytes"] += b
            for o, c in own.most_common(1):
                d["origins"][o] += b
    res = {p: {**{k: v for k, v in d.items() if k != "origins"}, "origins": d["origins"].most_common(5)}
           for p, d in pk.items()}
    json.dump(res, open(out, "w"), indent=0, sort_keys=True)
    car = [p for p, d in res.items() if d["copiedFiles"]]
    print(f"candidate-only packages {len(res)}; carriers (>=1 copied file) {len(car)}")


if __name__ == "__main__":
    main()
