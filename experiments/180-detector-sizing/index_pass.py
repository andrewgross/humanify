#!/usr/bin/env python3
"""exp180 step 1 (the ONE heavy read): stream both slices' unit row tables of the full index and write every print
held by at most MAX_OWNERS owners across the whole index (both slices together) as a detector CANDIDATE.

usage: <exp095 venv python> -I index_pass.py <index dir> <out dir>      (run under flock /work/heavy.lock)

NOT RUN on the full index for exp180's numbers: it was queued on /work/heavy.lock for seven hours behind evals and
index builds and then cancelled; it was checked on a 50,000-row prefix of both slices and a hand-built group.
pickle_pass.py wrote the same format from the scorer's pickled unit tables (see its docstring for the one
difference: the content column).

`units.<slice>.jsonl` is sorted by hash (asserted), so the two slices are merged by hash in one pass and nothing is
held in memory but one hash group. One row per (owner, print):

  owner hash nodes content covered indexed slices files minModuleBytes firstFile recent vmask nOwners owners

`covered` = how many of the owner's indexed versions (versions.json) hold the print in ANY slice and ANY file;
`indexed` = the owner's indexed version count; `recent` = how many of the owner's newest min(RECENT, indexed)
versions hold it; `vmask` = the covered versions as a hex bitmask over the owner's versions.json order (bit i =
version i); `slices` = 1 (bun-node) | 2 (bun-bun) | 3 (both); `nOwners`/`owners` = every owner of the print.
It also writes `owners.json`: per owner, rows, distinct prints, single-owner prints, indexed versions.
"""
import collections
import heapq
import json
import os
import sys

import orjson

IX, OUT = sys.argv[1], sys.argv[2]
SLICES = ("bun-node-cjs-min", "bun-bun-cjs-min")
VERSIONS = json.load(open(os.path.join(IX, "versions.json")))
POS = {o: {v: i for i, v in enumerate(vs)} for o, vs in VERSIONS.items()}
RECENT = 10
MAX_OWNERS = 3
HEADER = ("owner", "hash", "nodes", "content", "covered", "indexed", "slices", "files", "minModuleBytes",
          "firstFile", "recent", "vmask", "nOwners", "owners")


def rows(slice_i):
    last = ""
    with open(os.path.join(IX, f"units.{SLICES[slice_i]}.jsonl"), "rb") as fh:
        for line in fh:
            r = orjson.loads(line)
            h = r["hash"]
            if h < last:
                raise SystemExit(f"units.{SLICES[slice_i]}.jsonl is not sorted by hash at {h}")
            last = h
            yield h, slice_i, r


def expand(owner, ranges):
    pos = POS.get(owner, {})
    out = 0
    for lo, hi in ranges:
        if lo in pos and hi in pos:
            out |= ((1 << (pos[hi] - pos[lo] + 1)) - 1) << pos[lo]
    return out


def candidate_rows(h, group):
    """One TSV line per owner of a print held by at most MAX_OWNERS owners."""
    by_owner = collections.defaultdict(list)
    for si, r in group:
        by_owner[r["owner"]].append((si, r))
    if len(by_owner) > MAX_OWNERS:
        return []
    owners = ",".join(sorted(by_owner))
    lines = []
    for o, rs in sorted(by_owner.items()):
        cov, files, mask, nodes, content, mb = 0, set(), 0, 0, 0, None
        for si, r in rs:
            cov |= expand(o, r["versions"])
            files.add(r["file"])
            mask |= 1 << si
            nodes = max(nodes, r["nodes"])
            content = max(content, r["content"])
            mb = r["moduleBytes"] if mb is None else min(mb, r["moduleBytes"])
        n = len(VERSIONS.get(o, ()))
        recent = (cov >> max(n - RECENT, 0)).bit_count()
        lines.append(f"{o}\t{h}\t{nodes}\t{content}\t{cov.bit_count()}\t{n}\t{mask}\t{len(files)}\t{mb}\t"
                     f"{min(files)}\t{recent}\t{cov:x}\t{len(by_owner)}\t{owners}\n")
    return lines


def main():
    os.makedirs(OUT, exist_ok=True)
    stats = collections.defaultdict(lambda: [0, 0, 0])  # owner -> rows, distinct prints, single-owner prints
    groups = 0
    out = open(os.path.join(OUT, "candidates.tsv"), "w")
    out.write("\t".join(HEADER) + "\n")
    cur, group = None, []

    def flush():
        nonlocal groups
        if not group:
            return
        groups += 1
        owners = {r["owner"] for _, r in group}
        for o in owners:
            stats[o][1] += 1
            stats[o][2] += len(owners) == 1
        for _, r in group:
            stats[r["owner"]][0] += 1
        out.writelines(candidate_rows(cur, group))

    for h, si, r in heapq.merge(rows(0), rows(1), key=lambda x: x[0]):
        if h != cur:
            flush()
            cur, group = h, []
        group.append((si, r))
    flush()
    out.close()
    json.dump({"index": os.path.basename(os.path.normpath(IX)), "hashGroups": groups,
               "owners": {o: {"rows": s[0], "prints": s[1], "singleOwnerPrints": s[2],
                              "indexedVersions": len(VERSIONS.get(o, ()))} for o, s in sorted(stats.items())}},
              open(os.path.join(OUT, "owners.json"), "w"), indent=0, sort_keys=True)
    print(f"hash groups {groups}, owners {len(stats)}", flush=True)


if __name__ == "__main__":
    main()
