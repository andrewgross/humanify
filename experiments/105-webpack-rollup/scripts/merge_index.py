"""exp105: one index = a base index + the slices another index adds (e.g. step 4's Bun/esbuild slices + exp105's
webpack/rollup slices), without rebuilding either: a run reads ONE slice (the one its recipe picks) plus the shared
literal table, so the merged index answers a Bun or esbuild bundle exactly as the base does and a webpack or rollup
bundle exactly as the added index does — except for literals, which come from the BASE (its literal table covers the
base's builds; the added index's own literals are not merged).

usage: python3 -I merge_index.py <base index dir> <added index dir> <out dir>

The base's files are HARD-LINKED (never written: the new manifest is a new file); the added slices' tables are
copied. The manifest is the base's, with the added slices appended to `slices`, their tables' sha256 in `tables` and
their shard keys in `shards`. Refused: an added slice the base already has; indexes built with other fingerprint or
recipe versions; a version table (versions.json) the merge cannot keep (the base's is kept: the added rows' version
RANGES are over the added index's own versions — read by the scorer's exploratory block only, never by a lookup).
"""
import hashlib
import json
import os
import shutil
import sys


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main(base, added, out):
    mb = json.load(open(os.path.join(base, "manifest.json")))
    ma = json.load(open(os.path.join(added, "manifest.json")))
    for k in ("format", "asOf", "fingerprints"):
        if mb[k] != ma[k]:
            raise SystemExit(f"REFUSED: the indexes differ in {k}: {mb[k]} vs {ma[k]}")
    have = {s["slice"] for s in mb["slices"]}
    new = [s for s in ma["slices"] if s["slice"] not in have]
    if len(new) != len(ma["slices"]):
        raise SystemExit("REFUSED: the added index repeats a slice the base has")
    if os.path.exists(out):
        raise SystemExit(f"REFUSED: {out} exists")
    os.makedirs(out)
    for name in os.listdir(base):
        if name != "manifest.json":
            os.link(os.path.join(base, name), os.path.join(out, name))
    tables = dict(mb["tables"])
    shards = dict(mb["shards"])
    for s in new:
        for kind in ("units", "modules"):
            name = f"{kind}.{s['slice']}.jsonl"
            shutil.copyfile(os.path.join(added, name), os.path.join(out, name))
            tables[name] = sha(os.path.join(out, name))
        shards.update({k: v for k, v in ma["shards"].items() if k.startswith(s["slice"] + "/")})
    merged = dict(mb)
    merged["slices"] = mb["slices"] + new
    merged["tables"] = dict(sorted(tables.items()))
    merged["shards"] = dict(sorted(shards.items()))
    text = json.dumps(merged, indent=2, ensure_ascii=False) + "\n"
    open(os.path.join(out, "manifest.json"), "w").write(text)
    print(f"merged {len(new)} slice(s) into {out}: index id {hashlib.sha256(text.encode()).hexdigest()}")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    main(*sys.argv[1:])
