"""exp118: GENERIC candidate populations for growing the index, built only from public npm metadata and the index
itself (never from a scored set). Each population is a set of package names NOT already indexed.

usage: python3 -I populations.py <index dir> <data dir> frontier <level>   # names to fetch for closure level+1
       python3 -I populations.py <index dir> <data dir> build              # -> <data dir>/populations.json

Populations (every one minus the index's own owners):
  dependents-top{20k,50k,100k}   npm packages by dependent-package count (packages.ecosyste.ms)
  downloads-<lo>k-<hi>k          npm packages by last-month downloads, rank bands (0-16k ~ the top-16k exp095
                                 read from npm-high-impact; 16k-50k the band past it)
  closure-runtime-L1             dependencies / optionalDependencies / peerDependencies of EVERY indexed version
  closure-runtime-L<k>           ... plus the runtime deps of each level k-1 package's latest version
  closure-dev                    devDependencies of each indexed package's newest indexed version (what a package's
                                 own build may inline into its dist)
  closure-bundled                bundleDependencies of the newest indexed versions
  scopes                         every package in an @scope that holds an indexed package (sized from the ranked
                                 lists only: the registry has no scope listing)
  maintainers                    packages whose maintainers include a maintainer of an indexed package
  paths                          names that appear as a vendored directory in an indexed package's file paths
"""
import collections
import json
import os
import re
import sys

NAME = re.compile(r"^(@[a-z0-9][a-z0-9._~-]*/)?[a-z0-9][a-z0-9._~-]*$")
# publisher identities shared by unrelated packages (provenance publishing): never a maintainer signal. _npmUser is
# not read at all for the same reason (2,470 of the indexed newest versions were published by "GitHub Actions").
PSEUDO = {"github actions", "npm-oidc-no-reply", "github-actions"}
VENDOR_DIRS = {"vendor", "_vendor", "vendors", "vendored", "compiled", "third_party", "third-party", "thirdparty",
               "external", "externals", "deps", "bundled", "node_modules", "lib-vendor", "dist-vendor", "_virtual"}


def jl(p):
    with open(p) as f:
        return json.load(f)


def cache_body(path):
    if not os.path.exists(path):
        return None
    return jl(path).get("body")


def fn(name):
    return name.replace("/", "__") + ".json"


def indexed(ix):
    return jl(os.path.join(ix, "versions.json"))


def ranked(data, sort):
    d = os.path.join(data, "eco", sort + "-100")
    if not os.path.isdir(d):
        d = os.path.join(data, "eco", sort)
    rows = []
    for p in sorted(os.listdir(d)):
        if p.endswith(".json"):
            rows.extend(jl(os.path.join(d, p)))
    seen, out = set(), []
    for r in rows:  # pages can overlap at ties; keep first occurrence
        if r["name"] and r["name"] not in seen:
            seen.add(r["name"])
            out.append(r)
    return out


def dep_names(m):
    out = set()
    for k in ("dependencies", "optionalDependencies", "peerDependencies"):
        for n, spec in (m.get(k) or {}).items():
            if isinstance(spec, str) and spec.startswith("npm:"):
                n = spec[4:].rsplit("@", 1)[0] or n
            out.add(n)
    return out


def closure_levels(ix, data, upto):
    vers = indexed(ix)
    own = set(vers)
    levels = []
    l1 = set()
    for n, vs in vers.items():
        doc = cache_body(os.path.join(data, "npm", "abbr", fn(n)))
        if not doc:
            continue
        for v in vs:
            l1 |= dep_names(doc["versions"].get(v, {}))
    l1 -= own
    levels.append(l1)
    have = own | l1
    for _ in range(1, upto):
        nxt = set()
        for n in levels[-1]:
            doc = cache_body(os.path.join(data, "npm", "abbr", fn(n)))
            if not doc:
                continue
            latest = doc.get("dist-tags", {}).get("latest")
            nxt |= dep_names(doc["versions"].get(latest, {}))
        nxt -= have
        have |= nxt
        levels.append(nxt)
    return levels


def newest_manifests(ix, data):
    out = {}
    d = os.path.join(data, "npm", "ver")
    # indexed-newest.txt names the version fetched per owner (its newest indexed version)
    for line in open(os.path.join(data, "indexed-newest.txt")):
        nv = line.strip()
        if not nv:
            continue
        n = nv.rpartition("@")[0]
        out[n] = cache_body(os.path.join(d, fn(nv)))
    return out


def people(m):
    out = set()
    for x in (m.get("maintainers") or []):
        if isinstance(x, dict) and x.get("name"):
            out.add(x["name"].lower())
        elif isinstance(x, str):
            out.add(x.split(" <")[0].lower())
    return out - PSEUDO


def path_names(ix):
    names = set()
    files = set()
    for f in os.listdir(ix):
        if f == "literals.files.jsonl" or f.startswith("modules."):
            with open(os.path.join(ix, f)) as fh:
                for line in fh:
                    r = json.loads(line)
                    for p in ([r["file"]] if "file" in r else r.get("files", [])):
                        files.add(p)
    for p in files:
        seg = p.split("/")
        for i, s in enumerate(seg[:-1]):
            if s.lower() in VENDOR_DIRS and i + 1 < len(seg) - 1:
                a = seg[i + 1]
                cand = a + "/" + seg[i + 2] if a.startswith("@") and i + 2 < len(seg) - 1 else a
                if NAME.match(cand.lower()):
                    names.add(cand.lower())
    return names


def build(ix, data):
    vers = indexed(ix)
    own = set(vers)
    pops = {}
    dep = ranked(data, "dependent_packages_count")
    dl = ranked(data, "downloads")
    for k in (20000, 50000, 100000):
        if len(dep) >= k * 0.995:  # a few tied rows repeat across pages and are dropped
            pops[f"dependents-top{k // 1000}k"] = {r["name"] for r in dep[:k]} - own
    bands = [(0, 16000), (16000, 25000), (25000, 35000), (35000, 50000), (16000, 50000), (0, 50000), (50000, 100000)]
    for lo, hi in bands:
        if len(dl) >= hi * 0.995:
            pops[f"downloads-{lo // 1000}k-{hi // 1000}k"] = {r["name"] for r in dl[lo:hi]} - own
    levels = closure_levels(ix, data, 9)
    acc = set()
    for i, lv in enumerate(levels):
        if not lv and i:
            break
        acc |= lv
        pops[f"closure-runtime-L{i + 1}"] = set(acc)
    man = newest_manifests(ix, data)
    dev, bundled = set(), set()
    maint = collections.Counter()
    for n, m in man.items():
        if not m:
            continue
        dev |= set((m.get("devDependencies") or {}).keys())
        b = m.get("bundleDependencies") or m.get("bundledDependencies") or []
        if isinstance(b, dict):
            b = list(b)
        if isinstance(b, list):
            bundled |= {x for x in b if isinstance(x, str)}
        maint.update(people(m))
    pops["closure-dev"] = dev - own
    pops["closure-bundled"] = bundled - own
    scopes = {n.split("/")[0] for n in own if n.startswith("@")}
    ranked_all = {r["name"]: r for r in dep + dl}
    pops["scopes(ranked-only)"] = {n for n in ranked_all if n.startswith("@") and n.split("/")[0] in scopes} - own
    pops["maintainers(ranked-only)"] = {n for n, r in ranked_all.items()
                                        if {x.lower() for x in r.get("maintainers") or []} & set(maint)} - own
    pops["paths"] = path_names(ix) - own
    meta = {"indexedOwners": len(own), "indexedVersions": sum(len(v) for v in vers.values()),
            "indexedScopes": len(scopes), "indexedMaintainers": len(maint),
            "rankedDependents": len(dep), "rankedDownloads": len(dl),
            "dependentsAtRank": {str(k): dep[min(k, len(dep)) - 1]["dependents"] for k in (16000, 20000, 50000, 100000)
                                 if len(dep) >= k * 0.995},
            "downloadsAtRank": {str(k): dl[min(k, len(dl)) - 1]["downloads"] for k in (16000, 25000, 35000, 50000, 100000)
                                if len(dl) >= k * 0.995},
            "closureLevelSizes": [len(x) for x in levels]}
    return pops, meta, scopes, maint


def main():
    ix, data, mode = sys.argv[1], sys.argv[2], sys.argv[3]
    if mode == "frontier":
        level = int(sys.argv[4])
        lv = closure_levels(ix, data, level)
        need = [n for n in sorted(lv[-1]) if not os.path.exists(os.path.join(data, "npm", "abbr", fn(n)))]
        print("\n".join(need))
        print(f"level {level}: {len(lv[-1])} names, {len(need)} to fetch", file=sys.stderr)
        return
    pops, meta, scopes, maint = build(ix, data)
    with open(os.path.join(data, "populations.json"), "w") as f:
        json.dump({"meta": meta, "populations": {k: sorted(v) for k, v in pops.items()},
                   "scopes": sorted(scopes), "maintainers": dict(sorted(maint.items()))}, f)
    print(json.dumps({"meta": meta, "sizes": {k: len(v) for k, v in pops.items()}}, indent=1))


if __name__ == "__main__":
    main()
