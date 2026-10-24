"""exp148 step 1: main's WRONG-family bytes (scorer 14 verdict `wrongFamily`, both scopes) on the recommended index,
split by whether the index holds the truth's shipper package T at the bundled version V:

  a  not-indexed      T is in no version of the index (versions.json)
  b  version-missing  T is indexed, V is not
  c  held             T@V is indexed: matching went wrong with the truth available
  n  no-version       the truth row names no version (T indexed or not is still recorded)

For a/b also: is T a public npm package (registry snapshot has it), and for a the selection-gap reason (how many
training apps' closures bundle T; refused versions in the index manifest), for b the version-policy reason.

usage: python3 -I census.py <index dir> <registry dir>[,<dir>...] <closure counts.json> <out.json>
         <corpus>=<misses.jsonl> ...
Output rows name packages and stay OUTSIDE the repo; the printed totals are aggregates (held-out: no names)."""
import collections
import datetime
import json
import os
import re
import sys

AS_OF = datetime.datetime(2026, 10, 15, 23, 59, 59, 999000, tzinfo=datetime.timezone.utc)
HIDDEN = ("heldout",)


def parse(t):
    try:
        return datetime.datetime.fromisoformat(t.replace("Z", "+00:00"))
    except (AttributeError, ValueError):
        return None


class Registry:
    def __init__(self, dirs):
        self.dirs, self.cache = dirs, {}

    def doc(self, name):
        if name not in self.cache:
            d = None
            for root in self.dirs:
                p = os.path.join(root, name.replace("/", "__") + ".json")
                if os.path.exists(p):
                    d = json.load(open(p))
                    break
            self.cache[name] = d
        return self.cache[name]

    def public(self, name):
        d = self.doc(name)
        if d is None:  # every name was fetched (exp142's snapshot + this lane's): no file = registry 404
            return "not-on-npm"
        return "public" if d.get("time") or d.get("versions") else "not-on-npm"

    def time(self, name, v):
        d = self.doc(name) or {}
        return parse((d.get("time") or {}).get(v))


STORE = "/work/exp142-data/full/store/shards"  # the recommended index's shard store (exp142's hard-link copy)


def shard_error(p, v, slice_):
    """Why a PINNED version owns no rows: the shard's recorded build errors (the builder writes an empty shard when
    every entry build fails), else 'built-no-rows' (types-only, or every row credited to an origin)."""
    f = os.path.join(STORE, slice_, f"{p.replace('/', '__')}@{v}.json")
    if not os.path.exists(f):
        return "no-shard-file"
    builds = json.load(open(f)).get("notes", {}).get("builds") or {}
    errs = " ".join(builds.values())
    if not errs or errs.strip() == "ok ok" or all(b == "ok" for b in builds.values()):
        return "built-no-rows"
    m = re.search(r'No loader is configured for "(\.[A-Za-z0-9]+)"', errs)
    if m:
        return "no-loader-" + m.group(1)
    if "Could not resolve" in errs:
        return "could-not-resolve"
    return "build-error"


def gap_reason(r, versions, pins, refused, counts, reg):
    p, v = r["pkg"], r["version"]
    pub = reg.public(p)
    if pub != "public":
        return pub
    slice_ = r.get("slice") or "esbuild-node-cjs-min"
    if p not in versions:
        if p in pins:
            vv = v if v in pins[p] else max(pins[p])
            return "selected-no-rows:" + shard_error(p, vv, slice_)
        n = counts.get(p, 0)
        return f"apps={min(n, 2)}" if n < 2 else "apps>=2-not-selected"
    if v in refused.get(p, {}):
        return "version-refused:" + refused[p][v]
    if v in pins.get(p, ()):
        return "version-pinned-no-rows:" + shard_error(p, v, slice_)
    t = reg.time(p, v)
    if t is None:
        return "version-unknown-time"
    if t > AS_OF:
        return "version-after-asof"
    if "-" in v:
        return "version-not-chosen:prerelease"
    if (AS_OF - t).days <= 365:
        return "version-not-chosen:last-12-months"
    return "version-not-chosen:" + ("older-0.x" if v.startswith("0.") else "older")


def main():
    ix, regs, counts_path, out = sys.argv[1], sys.argv[2].split(","), sys.argv[3], sys.argv[4]
    versions = {k: set(v) for k, v in json.load(open(os.path.join(ix, "versions.json"))).items()}
    man = json.load(open(os.path.join(ix, "manifest.json")))
    pins = collections.defaultdict(set)
    for x in man["packages"]:
        pins[x["name"]].add(x["version"])
    refused = collections.defaultdict(dict)
    for x in man["refused"]:
        refused[x["name"]][x["version"]] = x["reason"]
    counts = json.load(open(counts_path))
    reg = Registry(regs)
    res, rows = {}, []
    for arg in sys.argv[5:]:
        corpus, path = arg.split("=", 1)
        for line in open(path):
            r = json.loads(line)
            if r["verdict"] != "wrongFamily":
                continue
            p, v = r["pkg"], r["version"]
            if v is None:
                cls = "n"
            elif p not in versions:
                cls = "a"
            elif v not in versions[p]:
                cls = "b"
            else:
                cls = "c"
            why = gap_reason(r, versions, pins, refused, counts, reg) if cls in ("a", "b") else ""
            d = res.setdefault(r["set"], collections.Counter())
            d["total"] += r["bytes"]
            d[cls] += r["bytes"]
            if why:
                d[cls + ":" + why] += r["bytes"]
            rows.append({"corpus": corpus, "set": r["set"], "entry": r["entry"], "bytes": r["bytes"],
                         "scope": r["scope"], "pkg": p, "version": v, "origin": r.get("origin"),
                         "claimed": r["claimed"], "cls": cls, "why": why, "trainingApps": counts.get(p, 0),
                         "file": r.get("file"), "slice": r.get("slice"), "w": (r.get("w") or [None])[0]})
    json.dump({"kb": {s: {k: round(b / 1024, 1) for k, b in sorted(c.items())} for s, c in res.items()}},
              open(out, "w"), indent=1, sort_keys=True)
    with open(out.replace(".json", ".rows.jsonl"), "w") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")
    for s in sorted(res):
        c = res[s]
        t = c["total"] or 1
        print(f"{s:16s} wrong {c['total'] / 1024:8.1f} KB | " + " ".join(
            f"{k}={c[k] / 1024:.1f} ({100 * c[k] / t:.0f}%)" for k in ("a", "b", "c", "n")))


if __name__ == "__main__":
    main()
