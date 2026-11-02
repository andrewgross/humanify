#!/usr/bin/env python3
"""exp180 step 1 as RUN: the detector candidates (index_pass.py's exact format) from the exp096 scorer's pickled unit
tables of index cb64c1f5 — the same unit rows (hash, owner, file, nodes, smallest module bytes, version ranges) the
scorer and exp167 read, already on disk, so the 43 GB index is not read at all.

usage: <exp095 venv python> -I pickle_pass.py <out candidates.tsv>
       <exp095 venv python> -I pickle_pass.py --fill-content <candidates.tsv> <out candidates.tsv> <reports dir>...

The pickles carry no `content` column (written -1: the content floor is not applied). `--fill-content` fills it
from the bundle reports' unit rows for every print that occurs in any of them — that is, for every print that can
ever produce a detector hit on the judged bundles — so the lookup's content floor is applied exactly wherever it can
change a result; the residue is a low-content print that never occurs in a judged bundle still occupying a pick
slot (index_pass.py, which reads `units.<slice>.jsonl` under /work/heavy.lock, removes it; it was queued for seven
hours behind evals and index builds and never ran).
"""
import collections
import glob
import json
import os
import pickle
import sys

CACHE = "/work/exp096-data/cache/ix-cb64c1f5fd507bee"
INDEX = "/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28"
RECENT, MAX_OWNERS = 10, 3  # index_pass.py's
HEADER = "owner\thash\tnodes\tcontent\tcovered\tindexed\tslices\tfiles\tminModuleBytes\tfirstFile\trecent\tvmask\tnOwners\towners\n"


def candidates(out_path):
    versions = json.load(open(os.path.join(INDEX, "versions.json")))
    pos = {o: {v: i for i, v in enumerate(vs)} for o, vs in versions.items()}

    def expand(o, ranges):
        p, m = pos.get(o, {}), 0
        for lo, hi in ranges:
            if lo in p and hi in p:
                m |= ((1 << (p[hi] - p[lo] + 1)) - 1) << p[lo]
        return m
    tables = [pickle.load(open(f"{CACHE}/ix-{s}.pickle", "rb"))["units"] for s in ("bun-node-cjs-min", "bun-bun-cjs-min")]
    with open(out_path, "w") as out:
        out.write(HEADER)
        for h in sorted(set(tables[0]) | set(tables[1])):
            by = collections.defaultdict(list)
            for si, t in enumerate(tables):
                for r in t.get(h, ()):
                    by[r[0]].append((si, r))
            if len(by) > MAX_OWNERS:
                continue
            owners = ",".join(sorted(by))
            for o, rs in sorted(by.items()):
                cov, files, nodes, mb, mask = 0, set(), 0, None, 0
                for si, r in rs:  # r = (owner, file, nodes, moduleBytes, version ranges)
                    cov |= expand(o, r[4])
                    files.add(r[1])
                    nodes = max(nodes, r[2])
                    mb = r[3] if mb is None else min(mb, r[3])
                    mask |= 1 << si
                n = len(versions.get(o, ()))
                recent = (cov >> max(n - RECENT, 0)).bit_count()
                out.write(f"{o}\t{h}\t{nodes}\t-1\t{cov.bit_count()}\t{n}\t{mask}\t{len(files)}\t{mb}\t{min(files)}\t"
                          f"{recent}\t{cov:x}\t{len(by)}\t{owners}\n")


def fill_content(src, dst, report_dirs):
    content = {}
    for d in report_dirs:
        for p in glob.glob(os.path.join(d, "*.json")):
            if p.endswith(".run.json"):
                continue
            lib = json.load(open(p))["libraryDiscovery"]
            for r in (lib or {}).get("unitRows", []):
                if "whole" in r:
                    content[r["whole"]["hash"]] = r["whole"]["content"]
                for f in r.get("functions", []):
                    content[f["hash"]] = f["content"]
    n = 0
    with open(src) as fh, open(dst, "w") as out:
        out.write(next(fh))
        for line in fh:
            f = line.split("\t")
            if f[1] in content:
                f[3] = str(content[f[1]])
                n += 1
            out.write("\t".join(f))
    print(f"content filled for {n} candidate rows", flush=True)


if __name__ == "__main__":
    if sys.argv[1] == "--fill-content":
        fill_content(sys.argv[2], sys.argv[3], sys.argv[4:])
    else:
        candidates(sys.argv[1])
