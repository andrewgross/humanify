"""exp114 lab: every PROPAGATED claim (down / module / glue) of a label with the unit's family covers (the
evidence-conflict guard's view, emulated: per family the nodes its prints cover, whole first, functions greedy, the
row floor, rivals under specificity) and its verdict. DEV corpora only.
usage: <venv python> -I proplab.py [--corpus C] <label> <out.jsonl>"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../106-matcher-hygiene"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv[1:])
import claims as C  # noqa: E402
import score  # noqa: E402
from libix import family  # noqa: E402

FLOOR = 30


def covers(ix, u, pkg):
    """{family: covered nodes}; the claimed package's family keyed '=own'."""
    w = u.get("w")
    prints = []
    if w and w[1] >= FLOOR:
        prints.append(("w", u["s"], u["e"], w[0], w[1]))
    for f in u.get("f", []):
        if f[3] >= FLOOR:
            prints.append(("f", f[0], f[1], f[2], f[3]))

    def holders(h):
        out = set()
        for r in ix.units.get(h, ()):
            if score.same_family(r[0], pkg):
                out.add("=own")
            elif r[3] <= 4 * u["mb"] + 4096:
                out.add(family(r[0]))
        return out
    cover = {}
    whole_held = holders(prints[0][3]) if prints and prints[0][0] == "w" else set()
    for f in whole_held:
        cover[f] = w[1]
    taken = collections.defaultdict(list)
    for p in sorted((p for p in prints if p[0] == "f"), key=lambda p: -p[4]):
        for f in holders(p[3]):
            if f in whole_held:
                continue
            if any(not (p[2] <= a or p[1] >= b) for a, b in taken[f]):
                continue
            taken[f].append((p[1], p[2]))
            cover[f] = cover.get(f, 0) + p[4]
    return cover


def main():
    label, out = ARGV[0], ARGV[1]
    om, cm = C.manifests()
    pipeline = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs = {}
    with open(out, "w") as fh:
        for e in C.entries():
            p = os.path.join(lib096.DATA, "results", label, "reports", e["id"] + ".json")
            if not os.path.exists(p):
                continue
            doc = json.load(open(p))
            rep = doc["libraryDiscovery"]
            if rep is None:
                continue
            sl = doc["toolchain"]["slice"]
            if sl not in ixs:
                ixs[sl] = score.load_index(pipeline["indexDir"], sl, pipeline["indexId"])
            ix = ixs[sl]
            units = {(u["s"], u["e"]): u for u in score.run_units(rep)}
            for r in C.entry_claims(label, e, om, cm).values():
                if r["rule"] == "seed":
                    continue
                u = units[(r["start"], r["end"])]
                row = dict(r)
                row["cover"] = covers(ix, u, r["package"]) if "w" in u or u.get("f") else {}
                row["nodes"] = u["w"][1] if "w" in u else 0
                fh.write(json.dumps(row) + "\n")


if __name__ == "__main__":
    main()
