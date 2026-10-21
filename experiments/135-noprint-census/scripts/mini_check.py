"""exp135: does a rebuild of the truth's own version under another recipe carry the wrong seed's unit? For every
census row (noprint.py's rows of DEV sets, keyed by truth package@version) read the rebuilt shard of the row's slice
and of the other slices in <store>, and count the unit's bytes whose prints the truth package now holds.

usage: python3 -I mini_check.py <mini-units.json> <store/shards> [--old <store/shards>]
prints KB per (set, census class/sub): rows, recovered (truth holds a print of the unit in the rebuilt slice)."""
import collections
import json
import os
import sys


def esc(p):
    return p.replace("/", "__")


def load(store, sl, pv):
    p, v = pv.rsplit("@", 1)
    f = os.path.join(store, sl, f"{esc(p)}@{v}.json")
    return json.load(open(f)) if os.path.exists(f) else None


def main():
    units, store = sys.argv[1], sys.argv[2]
    old = sys.argv[sys.argv.index("--old") + 1] if "--old" in sys.argv else None
    rows = json.load(open(units))
    tot, rec, rec_old = collections.Counter(), collections.Counter(), collections.Counter()
    for pv, rs in rows.items():
        p = pv.rsplit("@", 1)[0]
        cache = {}
        for r in rs:
            k = (r["set"], r["cls"], r["sub"].split("-other-slice")[0])
            tot[k] += r["bytes"]
            sl = r["slice"]
            if sl not in cache:
                d = load(store, sl, pv)
                cache[sl] = {u["hash"] for u in d.get("units", ()) if u["owner"] == p} if d else None
                o = load(old, sl, pv) if old else None
                cache[sl + "/old"] = {u["hash"] for u in o.get("units", ()) if u["owner"] == p} if o else set()
            if cache[sl] and set(r["hashes"]) & cache[sl]:
                rec[k] += r["bytes"]
            if set(r["hashes"]) & cache[sl + "/old"]:
                rec_old[k] += r["bytes"]
    print(f"{'set':14} {'class':18} {'sub':42} {'KB':>8} {'rebuilt':>8} {'old':>6}")
    for k in sorted(tot, key=lambda k: (k[0], -tot[k])):
        print(f"{k[0]:14} {k[1]:18} {k[2]:42} {tot[k] / 1e3:8.1f} {rec[k] / 1e3:8.1f} {rec_old[k] / 1e3:6.1f}")
    a, b = sum(tot.values()), sum(rec.values())
    print(f"TOTAL {a / 1e3:.1f} KB, truth holds the unit after the rebuild: {b / 1e3:.1f} KB ({100 * b / max(a, 1):.1f}%)")


if __name__ == "__main__":
    main()
