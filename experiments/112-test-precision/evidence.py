"""exp112: attach the index's own evidence to every wrong-family row: which owners (package, file, versions) hold
the claimed unit's WHOLE print in the run's index slice.

usage: python3 -I evidence.py <corpus-data-dir> <label> <index-dir> <rows.jsonl> <out.jsonl>
Streams the slice's units.<slice>.jsonl once per slice (only the hashes the wrong rows need)."""
import collections
import json
import os
import sys


def main():
    data, label, ix, rows_p, out_p = sys.argv[1:6]
    rows = [json.loads(l) for l in open(rows_p)]
    wrong = [r for r in rows if r["verdict"] == "wrongFamily"]
    by_entry = collections.defaultdict(list)
    for r in wrong:
        by_entry[r["entry"]].append(r)
    need = collections.defaultdict(set)
    for e, rs in by_entry.items():
        doc = json.load(open(os.path.join(data, "results", label, "reports", e + ".json")))
        sl = doc["toolchain"]["slice"]
        ur = {(u["start"], u["end"]): u for u in doc["libraryDiscovery"]["unitRows"]}
        for r in rs:
            u = ur.get((r["start"], r["end"]))
            r["slice"] = sl
            r["whole"] = (u or {}).get("whole")
            if r["whole"]:
                need[sl].add(r["whole"]["hash"])
    owners = {}
    for sl, hs in need.items():
        with open(os.path.join(ix, f"units.{sl}.jsonl")) as fh:
            for line in fh:
                h = line[9:25]
                if h in hs:
                    o = json.loads(line)
                    owners.setdefault((sl, h), []).append([o["owner"], o["file"], o["versions"]])
    with open(out_p, "w") as fh:
        for r in wrong:
            w = r.get("whole")
            r["owners"] = owners.get((r["slice"], w["hash"]), []) if w else []
            fh.write(json.dumps(r) + "\n")


if __name__ == "__main__":
    main()
