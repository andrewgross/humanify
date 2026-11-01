#!/usr/bin/env python3
"""exp180 step 4: how many versions per detected package a targeted index needs to keep today's claims.

usage: <exp095 venv python> -I versions.py <data dir> <out.json>

For every unit today's run NAMED (scorer-14 verdict right/family/origin), the claimed owner's rows that hold any of
the unit's prints in the bundle's own slice (the scorer's pickled unit table of index cb64c1f5) give the indexed
versions that support the claim. A targeted index that keeps only some versions of the owner keeps the claim when
one of them supports it. Two version pickers:

  latest-N   the owner's N newest indexed versions (the index is as-of 2026-10-25: for bun-dev, built that month
             from fresh installs, this is "the N versions around the bundle date"; Claude Code 2.1.216 is older)
  nearest-N  the N indexed versions nearest the TRUE bundled version (an oracle: a date-based pick can only
             approximate it), only for units whose true version is indexed

It also writes support-<corpus>.json.gz ({bundle: {unit position: hex mask of the supporting versions}}), which
simulate.py reads for its detector-driven version pick.

  vote-N     an IDEAL second stage: per (bundle, claimed owner), every named unit votes its bytes for each version
             supporting it, and the N versions with the most votes are kept (what fetching the detected package's
             full print list and voting would pick, without the wrong claims a real vote would also see)

A named unit with no row of its owner (a claim grown by propagation: DOWN/MODULE/glue, no print of its own) is
counted apart: it stands or falls with the seeds of its package, which this does not simulate.
"""
import collections
import gzip
import json
import os
import pickle
import sys

DATA, OUT = sys.argv[1], sys.argv[2]
CACHE = "/work/exp096-data/cache/ix-cb64c1f5fd507bee"
INDEX = "/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28"
NS = (1, 2, 3, 5, 10, 20)
NAMED = ("right", "family", "origin")
VERSIONS = json.load(open(os.path.join(INDEX, "versions.json")))
POS = {o: {v: i for i, v in enumerate(vs)} for o, vs in VERSIONS.items()}


def expand(owner, ranges):
    pos = POS.get(owner, {})
    out = set()
    for lo, hi in ranges:
        if lo in pos and hi in pos:
            out.update(range(pos[lo], pos[hi] + 1))
    return out


def main():
    tables = {}
    out = {}
    for corpus in ("bun-dev", "bun-held", "cc"):
        p = os.path.join(DATA, f"x-{corpus}.json.gz")
        if not os.path.exists(p):
            continue
        x = json.load(gzip.open(p, "rt"))
        r = collections.Counter()
        nver = collections.Counter()
        support = {}
        for b in x["bundles"]:
            sup_b = support[b["id"]] = {}
            votes = collections.defaultdict(collections.Counter)
            seeded = []
            sl = b["slice"]
            if sl not in tables:
                tables[sl] = pickle.load(open(os.path.join(CACHE, f"ix-{sl}.pickle"), "rb"))["units"]
            units = tables[sl]
            for pos, (bytes_, lab, ver, _o, _v, claim, verdict, prints) in enumerate(b["units"]):
                if verdict not in NAMED:
                    continue
                r["named"] += bytes_
                sup = set()
                for h in prints:
                    for row in units.get(h, ()):
                        if row[0] == claim:
                            sup |= expand(claim, row[4])
                if not sup:
                    r["grown"] += bytes_
                    continue
                r["seeded"] += bytes_
                sup_b[pos] = format(sum(1 << i for i in sup), "x")
                for i in sup:
                    votes[claim][i] += bytes_
                seeded.append((claim, sup, bytes_))
                n = len(VERSIONS.get(claim, ()))
                nver[claim] = n
                for k in NS:
                    if any(i >= n - k for i in sup):
                        r[f"latest{k}"] += bytes_
                tp = POS.get(claim, {}).get(ver) if claim == lab else None
                if tp is not None:
                    r["oracleBase"] += bytes_
                    for k in NS:
                        # the k indexed versions nearest the true one (ties toward newer)
                        near = sorted(range(n), key=lambda i: (abs(i - tp), -i))[:k]
                        if sup & set(near):
                            r[f"nearest{k}"] += bytes_
            for claim, sup, bytes_ in seeded:
                order = sorted(votes[claim], key=lambda i: (-votes[claim][i], -i))
                for k in NS:
                    if sup & set(order[:k]):
                        r[f"vote{k}"] += bytes_

        def sh(a, b):
            return round(100.0 * a / b, 2) if b else None
        out[corpus] = {
            "namedBytes": r["named"], "seededBytes": r["seeded"], "grownBytes": r["grown"],
            "latestN_shareOfSeeded": {k: sh(r[f"latest{k}"], r["seeded"]) for k in NS},
            "voteN_shareOfSeeded": {k: sh(r[f"vote{k}"], r["seeded"]) for k in NS},
            "nearestN_shareOfOracleBase": {k: sh(r[f"nearest{k}"], r["oracleBase"]) for k in NS},
            "oracleBaseBytes": r["oracleBase"],
            "claimedOwners": len(nver), "medianIndexedVersions": sorted(nver.values())[len(nver) // 2] if nver else None,
        }
        print(corpus, json.dumps(out[corpus]), flush=True)
        with gzip.open(os.path.join(DATA, f"support-{corpus}.json.gz"), "wt") as fh:
            json.dump(support, fh)
    json.dump(out, open(OUT, "w"), indent=1, sort_keys=True)


if __name__ == "__main__":
    main()
