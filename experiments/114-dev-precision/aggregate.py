"""exp114 lab: per index MODULE (owner, file) of one slice, how much of it is COPY evidence — rows whose print another
package family also holds in a module at most half this one's size (the smaller context is the likelier origin).
A module most of whose rows are such copies is an AGGREGATE (a package's pre-bundled dist of its dependencies).
usage: <venv python> -I aggregate.py <index dir> <slice> <out.json>"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../095-library-fingerprints/selection/scripts"))
from libix import family  # noqa: E402


def main():
    ix, sl, out = sys.argv[1:4]
    by_hash = collections.defaultdict(list)
    fam = {}
    with open(os.path.join(ix, f"units.{sl}.jsonl")) as fh:
        for line in fh:
            r = json.loads(line)
            o = r["owner"]
            if o not in fam:
                fam[o] = family(o)
            by_hash[r["hash"]].append((o, r["file"], r["nodes"], r["moduleBytes"]))
    mods = collections.defaultdict(lambda: [0, 0, 0, 0])  # rows, nodes, copy nodes, shared nodes
    for rows in by_hash.values():
        fams = {fam[r[0]] for r in rows}
        for o, f, n, mb in rows:
            m = mods[(o, f)]
            m[0] += 1
            m[1] += n
            if len(fams) > 1:
                m[3] += n
                if any(fam[o2] != fam[o] and 2 * mb2 <= mb for o2, _, _, mb2 in rows):
                    m[2] += n
    json.dump({f"{o}\t{f}": v for (o, f), v in mods.items()}, open(out, "w"))


if __name__ == "__main__":
    main()
