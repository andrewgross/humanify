"""exp110 LAB post-pass: the origin rule (crates/humanify-index/src/origin.rs) applied to an existing /3 index's unit
tables, for early numbers while /work/index-build.lock was held by another lane's build. The shipped rule is the
merge's; a --merge-only re-merge of the same store must give byte-identical unit tables (checked in the README).

usage: python3 -I postpass.py <index dir> <store> <work[:work...]> <out dir> <reencode binary>
Writes <out dir>: every file of <index dir> hard-linked, except units.<slice>.{jsonl,bin} (filtered, re-encoded),
dedup-postpass.json (counts), and manifest.json (the changed tables' sha256, so the index id changes).
"""
import collections
import hashlib
import json
import os
import re
import subprocess
import sys
from multiprocessing import Pool

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
from families_lib import Families, base_family  # noqa: E402

MAX_OWNERS = 3
FACTOR, SLACK = 4, 4096  # origin.rs ORIGIN_SPECIFICITY
FIELDS = ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies", "bundleDependencies",
          "bundledDependencies"]
OWN = re.compile(rb'"owner":"([^"]+)"')
STORE = None


def shard_owners(label):
    kind, rest = label.split("/", 1)
    pkg, ver = rest.rsplit("@", 1)
    path = os.path.join(STORE, "shards", kind, f"{pkg.replace('/', '__')}@{ver}.json")
    try:
        data = open(path, "rb").read()
    except FileNotFoundError:
        return pkg, set()
    return pkg, {o.decode() for o in OWN.findall(data)}


def declared(works, pkg, ver):
    for work in works.split(":"):
        p = os.path.join(work, f"{pkg.replace('/', '__')}@{ver}", "node_modules", pkg, "package.json")
        try:
            j = json.load(open(p))
            break
        except (OSError, ValueError):
            continue
    else:
        return None
    out = []
    for f in FIELDS:
        v = j.get(f)
        names = list(v) if isinstance(v, dict) else [x for x in v if isinstance(x, str)] if isinstance(v, list) else []
        out += [n for n in names if n not in out]
    return out


def main():
    global STORE
    ix, STORE, work, out, reencode = sys.argv[1:6]
    man = json.load(open(os.path.join(ix, "manifest.json")))
    deps = collections.defaultdict(set)
    with Pool(16) as p:
        for pkg, owners in p.imap_unordered(shard_owners, sorted(man["shards"]), chunksize=64):
            deps[pkg] |= owners - {pkg}
    ndecl = 0
    for r in man["packages"]:
        names = declared(work, r["name"], r["version"])
        if names is not None:
            ndecl += 1
            deps[r["name"]] |= set(names) - {r["name"]}
    fam = Families()

    def inliners(sizes):
        owners = sorted(sizes)
        if len(owners) < 2 or len(owners) > MAX_OWNERS:
            return {}
        res = {}
        for b in owners:
            for a in owners:
                if a != b and a in deps[b] and b not in deps[a] and sizes[a] <= FACTOR * sizes[b] + SLACK \
                        and not fam.same(a, b):
                    res[b] = a
                    break
        return {} if len(res) == len(owners) else res

    os.makedirs(out)
    record = {"rule": "origin/1 (lab post-pass)", "declaredVersions": ndecl, "slices": {}, "pairs": {}}
    pairs = collections.Counter()
    changed = []
    for name in sorted(os.listdir(ix)):
        if not (name.startswith("units.") and name.endswith(".jsonl")):
            continue
        slice_ = name[len("units."):-len(".jsonl")]
        src, dst = os.path.join(ix, name), os.path.join(out, name)
        hashes = rows_dropped = 0
        with open(src) as fh, open(dst, "w") as w:
            group, cur = [], None

            def flush():
                nonlocal hashes, rows_dropped
                sizes = {}
                for _, r in group:
                    sizes[r["owner"]] = min(sizes.get(r["owner"], 1 << 62), r["moduleBytes"])
                inl = inliners(sizes)
                if inl:
                    hashes += 1
                for line, r in group:
                    if r["owner"] in inl:
                        rows_dropped += 1
                        pairs[(r["owner"], inl[r["owner"]])] += 1
                    else:
                        w.write(line)
            for line in fh:
                r = json.loads(line)
                if r["hash"] != cur and group:
                    flush()
                    group = []
                cur = r["hash"]
                group.append((line, r))
            if group:
                flush()
        record["slices"][slice_] = {"hashes": hashes, "rows": rows_dropped}
        binname = f"units.{slice_}.bin"
        subprocess.check_call([reencode, dst, os.path.join(ix, "packages.json"), os.path.join(out, binname)])
        changed += [name, binname]
        print(slice_, hashes, rows_dropped, file=sys.stderr)
    for name in os.listdir(ix):
        if name not in changed and name != "manifest.json":
            os.link(os.path.join(ix, name), os.path.join(out, name))
    for name in changed:
        man["tables"][name] = hashlib.sha256(open(os.path.join(out, name), "rb").read()).hexdigest()
    with open(os.path.join(out, "manifest.json"), "w") as fh:
        json.dump(man, fh, indent=2)
        fh.write("\n")
    record["pairs"] = [{"inliner": a, "origin": b, "rows": n} for (a, b), n in pairs.most_common()]
    with open(os.path.join(out, "dedup-postpass.json"), "w") as fh:
        json.dump(record, fh, indent=1)


if __name__ == "__main__":
    main()
