"""exp107: every APP-CLAIMED statement unit of a label (truth <app>, claimed as any package), with what the source
map says the bytes are, for classifying each as a truth artifact or a matcher error. DEV-side corpora only (never
the test split: its per-bundle reports are not for reading).

usage: python3 -I app_claims.py <label> [--corpus dev|bundlers|apps] [--snippet N] [--set S]
"""
import bisect
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv)
if lib096.CORPUS == "test":
    raise SystemExit("REFUSED: the test split's per-bundle reports are not for reading")
import score  # noqa: E402
import truth_audit  # noqa: E402


def mapped_sources(entry, s, e, truth, smap_sources):
    segs = truth["segments"]
    offs = [x[0] for x in segs]
    k = max(bisect.bisect_right(offs, s) - 1, 0)
    c = collections.Counter()
    while k < len(segs) and segs[k][0] < e:
        a, b = max(segs[k][0], s), min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
        if b > a:
            c[smap_sources[segs[k][1]] if segs[k][1] < len(smap_sources) else "<maps to nothing>"] += b - a
        k += 1
    return c


def main():
    label = ARGV[1]
    snip = int(ARGV[ARGV.index("--snippet") + 1]) if "--snippet" in ARGV else 160
    only = ARGV[ARGV.index("--set") + 1] if "--set" in ARGV else None
    om, cm = lib096.load_origins_manifest(), truth_audit.load_copies_manifest()
    total = collections.Counter()
    for entry in lib096.load_manifest()["entries"]:
        if entry["truth"]["method"] != "sourcemap" or (only and entry["set"] != only):
            continue
        p = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
        if not os.path.exists(p):
            continue  # a diagnosis label may hold some sets only
        rep = json.load(open(p)).get("libraryDiscovery")
        if not rep:
            continue
        units = score.run_units(rep)
        st = score.statement_units(units)
        truth, origins = truth_audit.redecode(entry, lib096.load_truth(entry), lib096.load_origins(entry, om))
        labels = score.label_units(truth, units, origins, truth_audit.Audit(entry, truth, cm))
        key = {(u["s"], u["e"]): u for u in st}
        text = None
        srcs = None
        for c in rep.get("claims", []):
            u = key.get((c["start"], c["end"]))
            if u is None or labels[u["i"]]["label"] != "<app>":
                continue
            if text is None:
                text = open(lib096.corpus_path(entry, "bundle"), "rb").read()  # spans are UTF-8 byte offsets
                srcs = json.load(open(lib096.corpus_path(entry, "map")))["sources"]
            total[entry["set"]] += u["b"]
            ev = (c.get("evidence") or {}).get("kind", "")
            print(f"== {entry['id']} {c['start']}-{c['end']} {u['b']} B claimed {c['package']} rule {c['rule']} "
                  f"round {c['round']} evidence {ev}")
            for src, n in mapped_sources(entry, c["start"], c["end"], truth, srcs).most_common(4):
                print(f"     {n:>6} B  {src}")
            print("     | " + text[c["start"]:c["start"] + snip].decode("utf8", "replace").replace("\n", " "))
    print("TOTAL app-claimed by set:", dict(total))


if __name__ == "__main__":
    main()
