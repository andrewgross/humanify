"""exp136: the f / d sub-cause table (KB per set) from noprint.py --detail outputs (aggregate lines only: names never
appear in them). Groups noprint's detailed sub-causes into the lanes that own them.

usage: python3 -I fd_table.py <out.json> <census txt>...   prints a markdown table."""
import collections
import json
import sys

LANES = [
    # (lane, class, predicate on the detailed sub)
    ("f: truth has no version in its map (truth lane)", "f-filtered", lambda s: s.endswith("/no-version-in-map")),
    ("f: truth version not indexed (coverage)", "f-filtered", lambda s: s.endswith("/version-not-indexed")),
    ("f: spec bound, claimer holds it in a 4x smaller module (truth credits a shipper's inlined copy)", "f-filtered",
     lambda s: s.startswith("spec-bound/") and s.endswith("winner-smaller")),
    ("f: whole print read first, truth version indexed (build shape)", "f-filtered",
     lambda s: s.startswith("whole-read-first/") and "version" not in s),
    ("f: other (too many owners, spec bound comparable, other)", "f-filtered", lambda s: True),
    ("d: same file has rows, none of this unit's", "d-print-differs", lambda s: True),
]


def main():
    dest, files = sys.argv[1], sys.argv[2:]
    tab = collections.Counter()
    sets = set()
    for f in files:
        for line in open(f):
            if line.startswith("#") or "KB" not in line:
                continue
            kb, _, set_, cls, sub = line.split(None, 4)
            sub = sub.strip()
            if cls not in ("f-filtered", "d-print-differs"):
                continue
            sets.add(set_)
            for lane, c, pred in LANES:
                if c == cls and pred(sub):
                    tab[(lane, set_)] += float(kb)
                    break
    sets = sorted(sets)
    out = [{"lane": lane, **{s: round(tab[(lane, s)], 1) for s in sets}} for lane, _, _ in LANES]
    json.dump(out, open(dest, "w"), indent=1)
    print("| sub-cause | " + " | ".join(sets) + " |")
    print("|---|" + "--:|" * len(sets))
    for r in out:
        print(f"| {r['lane']} | " + " | ".join(f"{r[s]:.1f}" for s in sets) + " |")


if __name__ == "__main__":
    main()
