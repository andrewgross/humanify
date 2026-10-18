"""exp129: draw the hand-check sample: N units the copies-v2 layer relabelled (a ledger row: a claim whose verdict
changed), uniformly over distinct units of the DEV corpora, fixed seed; print each with the bundle's text and the
shipped source the map names, for a reader to judge whether the code is the claimed origin's.

usage: <venv python> -I handcheck_draw.py <seed> <n> <ledger rows.jsonl>... > draws.txt"""
import bisect
import json
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import truth_audit  # noqa: E402

CORPUS_OF_SET = {}


def corpus_of(entry_id):
    for c in lib096.CORPORA:
        if c in lib096.PRIVATE:
            continue
        lib096.use(c)
        if any(e["id"] == entry_id for e in lib096.load_manifest()["entries"]):
            return c
    raise SystemExit(f"{entry_id}: in no dev corpus")


def main():
    seed, n, paths = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3:]
    units = {}
    for p in paths:
        for line in open(p):
            r = json.loads(line)
            units.setdefault((r["entry"], r["s"], r["e"]), r)
    keys = sorted(units)
    random.Random(seed).shuffle(keys)
    for k in keys[:n]:
        r = units[k]
        lib096.use(corpus_of(r["entry"]))
        entry = next(e for e in lib096.load_manifest()["entries"] if e["id"] == r["entry"])
        text = open(lib096.corpus_path(entry, "bundle"), "rb").read()[r["s"]:r["e"]].decode("utf8", "replace")
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), truth_audit.load_chained_manifest())
        segs = truth["segments"]
        si = segs[bisect.bisect_right([s[0] for s in segs], r["s"]) - 1][1]
        mp = lib096.corpus_path(entry, "innerMap" if "innerMap" in entry["files"] else "map")
        m = json.load(open(mp))
        print("=" * 100)
        print(json.dumps({k2: r[k2] for k2 in ("entry", "s", "e", "b", "kind", "claim", "label_", "oldOrigin", "origin", "old", "new")}))
        print("source:", m["sources"][si] if si < len(m["sources"]) else "?")
        print("--- bundle text (first 700 chars):")
        print(text[:700])
        src = (m.get("sourcesContent") or [None] * (si + 1))[si] or ""
        origin = r["origin"] or r.get("oldOrigin") or ""
        print("--- shipped file mentions of the origin:", src.count(origin) if origin else 0)
        i = src.find(origin) if origin else -1
        print(src[max(0, i - 200):i + 300] if i >= 0 else "")


if __name__ == "__main__":
    main()
