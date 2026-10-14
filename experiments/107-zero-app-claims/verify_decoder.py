"""exp107: the scorer-6 decoder against the frozen truths, every source-mapped entry of every corpus.

usage: python3 -I verify_decoder.py [--jobs N] [--corpus dev,bundlers,apps,test]

Per entry: (1) decoder.segments with both rules OFF must equal the frozen truth's segments exactly (scorer 4's
decoder reproduced); (2) with both rules ON, truth_audit.redecode must accept it (the mapped segments are scorer 4's,
in order) — and the counts it changes: segments that moved (utf16) and one-field segments now kept (unmapped).
Prints per-corpus totals (and per entry for the dev-side corpora; never per entry for the test split).
"""
import json
import multiprocessing
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402


def one(job):
    corpus, entry = job
    lib096.use(corpus)
    import decoder
    import truth_audit
    truth = lib096.load_truth(entry)
    with open(lib096.corpus_path(entry, "bundle"), encoding="utf8") as fh:
        text = fh.read()  # scorer 4 read it with universal newlines
    smap = json.load(open(lib096.corpus_path(entry, "map")))
    inner = (json.load(open(lib096.corpus_path(entry, "innerMap"))), 1) if "innerMap" in entry["files"] else None
    old = decoder.segments(text, smap, inner, utf16=False, unmapped=False)
    same = [[s[0], s[1]] for s in old] == truth["segments"]
    om = lib096.load_origins_manifest()
    new, _ = truth_audit.redecode(entry, truth, lib096.load_origins(entry, om))
    mapped = [s for s in new["segments"] if s[1] != len(truth["sources"])]
    moved = sum(1 for a, b in zip(mapped, truth["segments"]) if a[0] != b[0])
    return corpus, entry["id"], same, len(truth["segments"]), moved, len(new["segments"]) - len(mapped)


def main():
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 8
    corpora = (sys.argv[sys.argv.index("--corpus") + 1] if "--corpus" in sys.argv else "dev,bundlers,apps,test")
    work = []
    for c in corpora.split(","):
        lib096.use(c)
        work += [(c, e) for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"]
    with multiprocessing.Pool(jobs) as pool:
        res = pool.map(one, work, chunksize=1)
    tot = {}
    for corpus, eid, same, n, moved, unm in res:
        t = tot.setdefault(corpus, [0, 0, 0, 0, 0, 0])
        t[0] += 1
        t[1] += not same
        t[2] += n
        t[3] += moved
        t[4] += unm
        t[5] += moved > 0
        if corpus != "test" and (moved or unm or not same):
            print(f"  {eid:60s} reproduced {same}  segments {n:>9,} moved {moved:>9,} unmapped kept {unm:>8,}")
    for c, t in tot.items():
        print(f"{c:9s} entries {t[0]:>4}  NOT reproduced {t[1]}  segments {t[2]:>12,}  moved {t[3]:>11,} "
              f"(in {t[5]} entries)  unmapped kept {t[4]:>10,}")
    sys.exit(1 if any(t[1] for t in tot.values()) else 0)


if __name__ == "__main__":
    main()
