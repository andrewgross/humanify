"""exp142: classify every MISSED library byte (misses.py rows with named = false) by whether the index could have
held the truth's (package, version), from the index's own versions.json and a registry snapshot's publish times.

  held               T@V is in the index (versions.json): the miss is not coverage
  after-asof         T is indexed, V is not, V was published after the index's as-of (2026-10-11 end of day)
                       sub: by-resolution (published on or before the corpus's dependency-resolution date) / later
  not-chosen         T is indexed, V is not, V was published on or before the as-of: the version-density rule did
                     not pick it.  sub: last-12-months / latest-patch-of-minor / older
  not-indexed        T is not in the index at all.  sub: V after the as-of / before / unknown time
  no-version         the truth row has no version (map names no version)
  unknown-time       V has no publish time in the snapshot (unpublished / private / snapshot gap)

Also per bundle the NEWEST publish time of any (T, V) its library units carry: a bundle cannot have been resolved
before it (its dependency-resolution floor).

usage: python3 -I classify.py <index dir> <registry dir>[,<dir>...] <out.json> <corpus>=<misses.jsonl> ...
Writes per-set KB per class/sub (named = 0 for held-out names) and, beside it, <out>.pkgs.json: per set the top
after-asof / not-chosen packages by KB (held-out excluded)."""
import collections
import datetime
import json
import os
import sys

AS_OF = datetime.datetime(2026, 10, 11, 23, 59, 59, 999000, tzinfo=datetime.timezone.utc)
# when each corpus's dependency trees were resolved (lock files / installs), read from the corpora's own scripts
# and confirmed by the per-bundle floors this script prints
HIDDEN = ("heldout",)


def parse(t):
    try:
        return datetime.datetime.fromisoformat(t.replace("Z", "+00:00"))
    except (AttributeError, ValueError):
        return None


def semver(v):
    core = v.split("+")[0]
    pre = "-" in core
    nums = core.split("-")[0].split(".")
    try:
        t = tuple(int(x) for x in nums[:3])
    except ValueError:
        return None
    return t + (0,) * (3 - len(t)) + (0 if pre else 1,) if len(t) <= 3 else None


class Registry:
    def __init__(self, dirs):
        self.dirs = dirs
        self.cache = {}

    def times(self, name):
        if name not in self.cache:
            t = {}
            for d in self.dirs:
                p = os.path.join(d, name.replace("/", "__") + ".json")
                if os.path.exists(p):
                    t = json.load(open(p)).get("time", {})
                    break
            self.cache[name] = {k: parse(v) for k, v in t.items() if k not in ("created", "modified")}
        return self.cache[name]


def policy_sub(times, v):
    t = {x: y for x, y in times.items() if y and y <= AS_OF and semver(x)}
    sv = semver(v)
    if (AS_OF - t[v]).days <= 365:
        return "last-12-months"
    same_minor = [x for x in t if semver(x)[:2] == sv[:2] and semver(x)[3] == 1]
    if same_minor and max(same_minor, key=semver) == v:
        return "latest-patch-of-minor"
    return "older"


def classify(r, versions, reg):
    p, v = r["pkg"], r["version"]
    if v is None:
        return "no-version", ""
    if p in versions and v in versions[p]:
        return "held", ""
    t = reg.times(p).get(v)
    if t is None:
        return ("not-indexed" if p not in versions else "unknown-time"), ("unknown-time" if p not in versions else "")
    after = t > AS_OF
    if p not in versions:
        return "not-indexed", "after-asof" if after else "before-asof"
    if after:
        return "after-asof", t.strftime("%Y-%m-%d")
    return "not-chosen", policy_sub(reg.times(p), v)


def main():
    ix, regs, out = sys.argv[1], sys.argv[2].split(","), sys.argv[3]
    versions = {k: set(v) for k, v in json.load(open(os.path.join(ix, "versions.json"))).items()}
    reg = Registry(regs)
    res, pkgs, floors = {}, {}, {}
    for arg in sys.argv[4:]:
        corpus, path = arg.split("=", 1)
        per = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
        top = collections.defaultdict(collections.Counter)
        newest = {}
        for line in open(path):
            r = json.loads(line)
            s = r["set"]
            per[s]["library"]["total"] += r["bytes"]
            if r["version"] is not None:
                t = reg.times(r["pkg"]).get(r["version"])
                if t is not None and (r["entry"] not in newest or t > newest[r["entry"]][0]):
                    newest[r["entry"]] = (t, s)
            if r["named"]:
                per[s]["named"]["total"] += r["bytes"]
                continue
            c, sub = classify(r, versions, reg)
            how = "wrongClaimed" if r["claimed"] else "unclaimed"
            per[s][c][sub or "-"] += r["bytes"]
            per[s][c + ":" + how]["-"] += r["bytes"]
            if r.get("origin") and r["origin"] in versions:
                per[s][c + ":originIndexed"]["-"] += r["bytes"]
            if c in ("after-asof", "not-chosen", "not-indexed") and s not in HIDDEN:
                top[(s, c)][f"{r['pkg']}@{r['version']}" + (f" ({sub})" if sub else "")] += r["bytes"]
        for s, d in per.items():
            res[s] = {"corpus": corpus, **{c: {k: round(b / 1024, 1) for k, b in sorted(cnt.items())}
                                           for c, cnt in sorted(d.items())}}
        for (s, c), cnt in top.items():
            pkgs.setdefault(s, {})[c] = [[k, round(b / 1024, 1)] for k, b in cnt.most_common(15)]
        by_set = collections.defaultdict(list)
        for e, (t, s) in newest.items():
            by_set[s].append(t)
        for s, ts in by_set.items():
            ts.sort()
            floors[s] = {"bundles": len(ts), "min": ts[0].isoformat()[:16], "median": ts[len(ts) // 2].isoformat()[:16],
                         "max": ts[-1].isoformat()[:16], "afterAsOf": sum(1 for t in ts if t > AS_OF)}
    json.dump({"asOf": AS_OF.isoformat(), "kb": res, "resolutionFloors": floors}, open(out, "w"), indent=1,
              sort_keys=True)
    json.dump(pkgs, open(out + ".pkgs.json", "w"), indent=1, sort_keys=True)
    for s in sorted(res):
        d = res[s]
        tot = d["library"]["total"]
        named = d.get("named", {}).get("total", 0)
        line = " ".join(f"{c}={sum(v.values()):.0f}" for c, v in d.items()
                        if c in ("held", "after-asof", "not-chosen", "not-indexed", "no-version", "unknown-time"))
        print(f"{s:16s} lib {tot:9.0f} KB named {named:9.0f} missed {tot - named:8.0f} | {line} | floor {floors.get(s)}")


if __name__ == "__main__":
    main()
