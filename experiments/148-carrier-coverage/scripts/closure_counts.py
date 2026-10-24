"""exp148: how many TRAINING apps' closures bundle each package — exp108 select_roots.py's count (exp095's training
apps + exp108's apps, held-out excluded), the number the index's package rule (>= 2 apps) reads. Writes
{package: apps} and, with a second path, {package: {version: apps}}.

usage: python3 -I closure_counts.py <out counts.json> [<out versions.json>]"""
import collections
import json
import os
import sys

SEL095 = "/work/exp095-library-fingerprints/selection"
SEL108 = "/work/exp108-data"


def main():
    held = set(json.load(open(f"{SEL095}/selection.json"))["heldout"])
    pk = collections.Counter()
    pv = collections.defaultdict(collections.Counter)
    for sel in (SEL095, SEL108):
        for slug in sorted(os.listdir(f"{sel}/apps")):
            p = f"{sel}/apps/{slug}/closure.json"
            if not os.path.exists(p):
                continue
            c = json.load(open(p))
            if c["id"] in held or not c.get("packages"):
                continue
            for name, info in c["packages"].items():
                pk[name] += 1
                if isinstance(info, dict) and info.get("version"):
                    pv[name][info["version"]] += 1
    json.dump(pk, open(sys.argv[1], "w"))
    if len(sys.argv) > 2:
        json.dump(pv, open(sys.argv[2], "w"))
    print(len(pk), "packages;", sum(1 for c in pk.values() if c >= 2), "bundled by >= 2 apps")


if __name__ == "__main__":
    main()
