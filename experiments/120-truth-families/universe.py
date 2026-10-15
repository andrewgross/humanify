"""exp120: every package NAME the truth of every corpus labels anything with, merged into the families universe.

usage: python3 -I universe.py <base universe.json> <out universe.json>
Reads, for each corpus (dev, apps, bundlers, pop, test, test2): the frozen truths' source tables (and a sample
truth's row packages), the frozen chained-sources layers (scorer 5, plus scorer 9's when frozen), the scorer 7
origin layers' names and the copies layers' origins. Only names are collected, never verdicts or per-app results;
for the private corpora (test, test2) it prints counts only. Writes {"names": base ∪ found, "pv": []}, sorted, the
input of exp104's fetch_metadata.py and derive_families.py (rules unchanged)."""
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402

CORPORA = ("dev", "apps", "bundlers", "pop", "test", "test2")


def is_pkg(n):
    return isinstance(n, str) and n and not n.startswith("<") and n != "?"


def layer(entry, suffix):
    p = os.path.join(lib096.DATA, "truth", entry["id"] + suffix)
    if not os.path.exists(p):
        return None
    with gzip.open(p, "rt") as fh:
        return json.load(fh)


def corpus_names(corpus):
    lib096.use(corpus)
    names = set()
    for e in lib096.load_manifest()["entries"]:
        t = lib096.load_truth(e)
        names.update(r[0] for r in t.get("sources", []) if is_pkg(r[0]))
        names.update(r["pkg"] for r in t.get("rows", []) if is_pkg(r.get("pkg")))
        for suffix in (".chained.json.gz", ".chained9.json.gz"):
            ch = layer(e, suffix)
            if ch:
                names.update(r[0] for r in ch["sources"].values() if is_pkg(r[0]))
        o7 = layer(e, ".origins7.json.gz")
        if o7:
            names.update(n for n in o7.get("names", []) if is_pkg(n))
        cp = layer(e, ".copies.json.gz")
        if cp:
            names.update(r[3] for r in cp.get("ranges", []) if is_pkg(r[3]))
    return names


def main():
    base_p, out_p = sys.argv[1:3]
    base = set(json.load(open(base_p))["names"])
    found = set()
    for c in CORPORA:
        n = corpus_names(c)
        print(f"{c:9s} names {len(n):6d}  not in base {len(n - base):5d}")
        found |= n
    allnames = sorted(base | found)
    print(f"base {len(base)}  new {len(found - base)}  total {len(allnames)}")
    open(out_p, "w").write(json.dumps({"names": allnames, "pv": []}) + "\n")


if __name__ == "__main__":
    main()
