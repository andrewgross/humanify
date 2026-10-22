"""exp135: how much of a slice the `production` recipe changes — per version, the OLD shard (exp130's store) against
the NEW one (this build's store): versions whose unit-row hash set differs, rows only in the new / only in the old,
and the files only one side's rows name (development vs production builds). Package names are not printed.

usage: python3 -I diff_shards.py <old store/shards> <new store/shards> <slice> [--jobs N]"""
import collections
import json
import multiprocessing
import os
import re
import sys

DEV = re.compile(r"(^|[./_-])(development|dev)([./_-]|$)")
PROD = re.compile(r"(^|[./_-])(production|prod)([./_-]|$)")


def rows(path):
    if not os.path.exists(path):
        return None
    d = json.load(open(path))
    return {(u["owner"], u["file"], u["hash"]) for u in d.get("units") or ()}


def one(args):
    old, new = args
    a, b = rows(old), rows(new)
    if a is None or b is None:
        return None
    only_new, only_old = b - a, a - b
    files_new = {(o, f) for o, f, _ in only_new} - {(o, f) for o, f, _ in a}
    files_old = {(o, f) for o, f, _ in only_old} - {(o, f) for o, f, _ in b}
    return (len(a), len(b), len(only_new), len(only_old),
            sum(1 for _, f in files_new if PROD.search(f)), sum(1 for _, f in files_old if DEV.search(f)),
            len(files_new), len(files_old))


def main():
    old, new, sl = sys.argv[1:4]
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 8
    names = sorted(os.listdir(os.path.join(new, sl)))
    work = [(os.path.join(old, sl, n), os.path.join(new, sl, n)) for n in names]
    t = collections.Counter()
    with multiprocessing.Pool(jobs) as pool:
        for r in pool.imap_unordered(one, work, chunksize=64):
            if r is None:
                t["unpaired"] += 1
                continue
            t["versions"] += 1
            t["changed"] += r[2] > 0 or r[3] > 0
            for k, v in zip(("rowsOld", "rowsNew", "rowsOnlyNew", "rowsOnlyOld", "prodFilesNew", "devFilesGone",
                             "filesOnlyNew", "filesOnlyOld"), r):
                t[k] += v
    print(json.dumps({"slice": sl, **t}, indent=1))


if __name__ == "__main__":
    main()
