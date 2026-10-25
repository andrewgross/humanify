"""exp151: what the edge tables cost and what they separate.

usage: python3 -I measure.py <ix with edges> <ix without edges> <edges store> > measure.json

- size: every file of both indexes; the edge files' share.
- coverage: per slice, unit rows with >= 1 recorded callee (INSIDE / OUT-OF / either).
- collisions: unit prints held by more than one row, split by kind (several owners; one owner, several files);
  for each, whether the rows' recorded callee sets differ (any pair / every row distinct), on the row table
  (per file) and, for several owners, per owner (the union of an owner's rows).
- representation: the bytes of the list table as built vs a set-hash column (8 bytes per keyed row) and both.
- unrecorded: the shards' notes on references that record no callee (container, unowned) vs recorded ones.
"""
import collections
import json
import os
import sys


def rows(path):
    with open(path) as f:
        for line in f:
            if line.strip():
                yield json.loads(line)


def sizes(ix):
    return {n: os.path.getsize(os.path.join(ix, n)) for n in sorted(os.listdir(ix))}


def sig(e):
    if e is None:
        return ()
    return (tuple(e["inside"]), tuple((c["owner"], c["hash"]) for c in e["outOf"]))


def slice_stats(ix, sl):
    edges = {(e["hash"], e["owner"], e["file"]): e for e in rows(os.path.join(ix, f"edges.{sl}.jsonl"))}
    by_hash = collections.defaultdict(list)
    n = n_any = n_in = n_out = 0
    callees = 0
    for u in rows(os.path.join(ix, f"units.{sl}.jsonl")):
        n += 1
        e = edges.get((u["hash"], u["owner"], u["file"]))
        if e:
            n_any += 1
            n_in += bool(e["inside"])
            n_out += bool(e["outOf"])
            callees += len(e["inside"]) + len(e["outOf"])
        by_hash[u["hash"]].append((u["owner"], u["file"], e))
    kinds = collections.Counter()
    for h, rs in by_hash.items():
        if len(rs) < 2:
            continue
        owners = {o for o, _, _ in rs}
        kind = "severalOwners" if len(owners) > 1 else "oneOwnerSeveralFiles"
        sigs = [sig(e) for _, _, e in rs]
        kinds[(kind, "groups")] += 1
        if len(set(sigs)) > 1:
            kinds[(kind, "differ")] += 1
        if len(set(sigs)) == len(sigs):
            kinds[(kind, "allDistinct")] += 1
        nonempty = [s for s in sigs if s != ()]
        if len(set(nonempty)) > 1:
            kinds[(kind, "differAmongRowsWithEdges")] += 1
        if not nonempty:
            kinds[(kind, "noRowHasEdges")] += 1
        if kind == "severalOwners":
            per_owner = collections.defaultdict(lambda: (set(), set()))
            for o, _, e in rs:
                if e:
                    per_owner[o][0].update(e["inside"])
                    per_owner[o][1].update((c["owner"], c["hash"]) for c in e["outOf"])
            osigs = [(frozenset(per_owner[o][0]), frozenset(per_owner[o][1])) for o in owners]
            if len(set(osigs)) > 1:
                kinds[(kind, "ownersDiffer")] += 1
            if len(set(osigs)) == len(osigs):
                kinds[(kind, "ownersAllDistinct")] += 1
    coll = collections.defaultdict(dict)
    for (k, m), v in sorted(kinds.items()):
        coll[k][m] = v
    keyed = len(edges)
    lists = len({sig(e) for e in edges.values()})
    return {
        "unitRows": n,
        "rowsWithAnyEdge": n_any,
        "rowsWithInside": n_in,
        "rowsWithOutOf": n_out,
        "shareAny": round(n_any / n, 4) if n else 0,
        "shareInside": round(n_in / n, 4) if n else 0,
        "callees": callees,
        "keyedRows": keyed,
        "distinctLists": lists,
        "collisions": coll,
    }


def representation(ix, sl, st):
    built = os.path.getsize(os.path.join(ix, f"edges.{sl}.bin"))
    k = st["keyedRows"]
    fence = (k + 63) // 64
    hash_only = 8 + 2 * 8 + (fence + k) * 8 + k * 8  # header, key column, one set hash per keyed row
    return {"listTableBytes": built, "setHashOnlyBytes": hash_only, "bothBytes": built + k * 8}


def unrecorded(store):
    out = collections.Counter()
    root = os.path.join(store, "shards")
    for sl in sorted(os.listdir(root)):
        if sl == "literals":
            continue
        for name in os.listdir(os.path.join(root, sl)):
            with open(os.path.join(root, sl, name)) as f:
                shard = json.load(f)
            for why, n in shard["notes"].get("unrecordedEdges", {}).items():
                out[(sl, why)] += n
            for u in shard["units"]:
                out[(sl, "recordedOnRows")] += len(u.get("inside", [])) + len(u.get("outOf", []))
    res = collections.defaultdict(dict)
    for (sl, why), n in sorted(out.items()):
        res[sl][why] = n
    return res


def main():
    ix, base, store = sys.argv[1:4]
    a, b = sizes(ix), sizes(base)
    edge_files = {n: s for n, s in a.items() if n.startswith("edges.")}
    with open(os.path.join(ix, "manifest.json")) as f:
        man = json.load(f)
    slices = [s["slice"] for s in man["slices"]]
    stats = {sl: slice_stats(ix, sl) for sl in slices}
    same = {}
    for n in sorted(set(a) & set(b)):
        if n.startswith(("units.", "modules.", "literals.", "packages.", "versions.", "helpers.")):
            with open(os.path.join(ix, n), "rb") as f1, open(os.path.join(base, n), "rb") as f2:
                same[n] = f1.read() == f2.read()
    print(json.dumps({
        "format": man["format"],
        "bytes": {"withEdges": sum(a.values()), "withoutEdges": sum(b.values()),
                  "edgeFiles": edge_files, "edgeBin": sum(s for n, s in edge_files.items() if n.endswith(".bin")),
                  "edgeJsonl": sum(s for n, s in edge_files.items() if n.endswith(".jsonl")),
                  "lookupWith": sum(s for n, s in a.items() if n.endswith(".bin")),
                  "lookupWithout": sum(s for n, s in b.items() if n.endswith(".bin"))},
        "sharedTablesByteIdentical": same,
        "slices": stats,
        "representation": {sl: representation(ix, sl, stats[sl]) for sl in slices},
        "unrecorded": unrecorded(store),
    }, indent=1, sort_keys=True))


main()
