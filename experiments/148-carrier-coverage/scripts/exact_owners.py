"""exp148: for every WRONG unit (census rows joined back to misses.py rows for the unit's whole print), which index
owners hold that WHOLE print in the slice the bundle reads (an exact, verbatim copy) — streamed once per slice from
units.<slice>.jsonl. Output {slice: {hash: [owner, ...]}} (package names only; stays outside the repo).

usage: python3 -I exact_owners.py <index dir> <out.json> <misses.jsonl> ..."""
import collections
import json
import os
import sys


def main():
    ix, out = sys.argv[1], sys.argv[2]
    need = collections.defaultdict(set)
    for path in sys.argv[3:]:
        for line in open(path):
            if '"wrongFamily"' not in line:
                continue
            r = json.loads(line)
            if r["verdict"] == "wrongFamily" and r.get("w"):
                need[r["slice"]].add(r["w"][0])
    res = {}
    for slice_, hashes in need.items():
        got = collections.defaultdict(set)
        with open(os.path.join(ix, f"units.{slice_}.jsonl")) as fh:
            for line in fh:
                h = line[9:25]
                if h in hashes:
                    got[h].add(json.loads(line)["owner"])
        res[slice_] = {h: sorted(o) for h, o in got.items()}
        print(slice_, len(hashes), "wanted", len(got), "held", flush=True)
    json.dump(res, open(out, "w"))


if __name__ == "__main__":
    main()
