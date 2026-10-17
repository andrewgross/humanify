"""exp122: which index build (shard root) holds each owner@version, per slice — read off the k2-all store's shards.

usage: python3 -I shard_owners.py <store>/shards <out dir> [jobs]
writes <out>/owners-<slice>.json: {"<owner>@<version>": [shard root, ...]} (index data only, no corpus content)
"""
import json
import multiprocessing
import os
import re
import sys

PAT = re.compile(rb'"owner":"([^"]+)","version":"([^"]+)"')


def one(path):
    with open(path, "rb") as f:
        b = f.read()
    i = b.find(b'"units":')
    return os.path.basename(path)[:-5], sorted({(o.decode(), v.decode()) for o, v in PAT.findall(b, i if i >= 0 else 0)})


def main():
    shards, out = sys.argv[1], sys.argv[2]
    jobs = int(sys.argv[3]) if len(sys.argv) > 3 else 16
    os.makedirs(out, exist_ok=True)
    with multiprocessing.Pool(jobs) as pool:
        for sl in sorted(os.listdir(shards)):
            if sl == "literals":
                continue
            d = os.path.join(shards, sl)
            files = [os.path.join(d, f) for f in sorted(os.listdir(d))]
            res = {}
            for root, ovs in pool.imap_unordered(one, files, chunksize=64):
                for o, v in ovs:
                    res.setdefault(f"{o}@{v}", []).append(root)
            for k in res:
                res[k].sort()
            json.dump(res, open(os.path.join(out, f"owners-{sl}.json"), "w"), sort_keys=True)
            print(sl, len(files), "shards", len(res), "owner-versions", flush=True)


if __name__ == "__main__":
    main()
