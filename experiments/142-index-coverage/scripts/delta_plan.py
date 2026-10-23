"""exp142: the DELTA of an as-of move: every version the index's own version policies pick at a NEW as-of that the
index does not already hold, for the packages already in it (manifest.json `packages` roles: `recent`/`minor` (roots) -> the builder's
year-all+minor; `pinned` -> exp101 tail_pins' year-last12+major4, the policy exp108's pins-k2 were drawn with).
Corpus-blind: reads only the index and a registry snapshot.

usage: python3 -I delta_plan.py <index dir> <registry dir> <new as-of YYYY-MM-DD> <out pins.txt>
Writes name@version specs (to pass as --pinned, so no policy re-pick can drop an old version) and prints counts."""
import collections
import datetime
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../101-index-coverage/scripts"))
import tail_pins  # noqa: E402


def year_all_minor(doc, as_of):
    """humanify-index's `year-all+minor` (asof.rs): every stable release of the 12 months before as-of + the
    latest patch of every older minor."""
    vs = tail_pins.stable(doc, as_of)
    since = as_of - datetime.timedelta(days=365)
    keep, minors = set(), set()
    for s, v, t in vs:
        if t > since:
            keep.add(v)
        elif (s[0], s[1]) not in minors:
            keep.add(v)
        minors.add((s[0], s[1]))
    return keep


def main():
    ix, reg, as_of, out = sys.argv[1:5]
    old_asof = datetime.datetime.fromisoformat(json.load(open(os.path.join(ix, "manifest.json")))["asOf"]
                                               .replace("Z", "+00:00"))
    as_of = datetime.datetime.fromisoformat(as_of + "T23:59:59.999+00:00")
    versions = {k: set(v) for k, v in json.load(open(os.path.join(ix, "versions.json"))).items()}
    roles = collections.defaultdict(set)
    for p in json.load(open(os.path.join(ix, "manifest.json")))["packages"]:
        roles[p["name"]].add(p["role"])
    specs, n = [], collections.Counter()
    for name in sorted(roles):
        p = os.path.join(reg, name.replace("/", "__") + ".json")
        if not os.path.exists(p):
            n["noPackument"] += 1
            continue
        doc = json.load(open(p))
        pick = set(tail_pins.pick(doc, as_of, "year-last12+major4"))
        if roles[name] & {"recent", "minor"}:
            pick |= year_all_minor(doc, as_of)
        times = {v: datetime.datetime.fromisoformat(t.replace("Z", "+00:00")) for v, t in doc["time"].items()
                 if v in pick}
        for v in sorted(pick - versions.get(name, set())):
            k = "afterOldAsOf" if times[v] > old_asof else "beforeOldAsOf"
            n[k] += 1
            if k == "afterOldAsOf":  # the as-of move only: an older version the policy did not pick stays out
                specs.append(f"{name}@{v}")
    open(out, "w").write("\n".join(specs) + "\n")
    print(json.dumps({"newAsOf": as_of.isoformat(), "packages": len(roles), "specs": len(specs),
                      "packagesTouched": len({s.rsplit('@', 1)[0] for s in specs}), **n}))


if __name__ == "__main__":
    main()
