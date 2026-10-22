"""exp135: per slice, how many indexed versions' shards record a FAILED build (notes.builds: req / dyn) and why
(error kind, no paths). Reads only each shard's tail (its `notes` object is last).

usage: python3 -I scan_builds.py <store/shards dir> <out.json> [--jobs N]
out.json: {slice: {"versions": n, "bothFailed": n, "oneFailed": n, "kinds": {kind: n}},
           "failed": {slice: [package@version, ...]}} (both builds failed)"""
import collections
import json
import multiprocessing
import os
import re
import sys

SLICES = ("bun-bun-cjs-min", "bun-node-cjs-min", "esbuild-node-cjs-min", "rollup-node-cjs-min",
          "webpack-node-cjs-min")


def kind(e):
    m = re.search(r'No loader is configured for "(\.[A-Za-z0-9]+)"', e)
    if m:
        return "no-loader" + m.group(1)
    if "Could not resolve" in e or "Cannot find module" in e or "Could not find" in e or "Can't resolve" in e:
        return "unresolved-import"
    if "timed out" in e or "timeout" in e.lower():
        return "timeout"
    return "other"


def notes(path):
    with open(path, "rb") as fh:
        size = os.fstat(fh.fileno()).st_size
        for back in (4096, 65536, 1 << 22, size):
            fh.seek(max(0, size - back))
            tail = fh.read().decode("utf8", "replace")
            k = tail.rfind('"notes":')
            if k >= 0:
                try:
                    return json.loads(tail[k + 8:tail.rstrip().rfind("}")])  # the root object's last brace
                except ValueError:
                    pass
            if back >= size:
                break
    return json.load(open(path)).get("notes") or {}


def one(arg):
    sl, path = arg
    try:
        n = notes(path)
    except (OSError, ValueError):
        return sl, os.path.basename(path)[:-5], None
    return sl, os.path.basename(path)[:-5], n.get("builds") or {}


def main():
    root, out = sys.argv[1], sys.argv[2]
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 16
    work = [(sl, os.path.join(root, sl, f)) for sl in SLICES for f in os.listdir(os.path.join(root, sl))]
    res = {sl: {"versions": 0, "bothFailed": 0, "oneFailed": 0, "unread": 0, "kinds": collections.Counter()}
           for sl in SLICES}
    failed = collections.defaultdict(list)
    with multiprocessing.Pool(jobs) as pool:
        for sl, pv, b in pool.imap_unordered(one, work, chunksize=256):
            r = res[sl]
            r["versions"] += 1
            if b is None:
                r["unread"] += 1
                continue
            bad = {k: e for k, e in b.items() if e != "ok"}
            if b and len(bad) == len(b):
                r["bothFailed"] += 1
                r["kinds"][kind(" ".join(bad.values()))] += 1
                failed[sl].append(pv)
            elif bad:
                r["oneFailed"] += 1
    for sl in res:
        res[sl]["kinds"] = dict(res[sl]["kinds"].most_common())
        failed[sl].sort()
    json.dump({"slices": res, "failed": failed}, open(out, "w"), indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
