#!/usr/bin/env python3
"""exp177: the "installed versions" rule — which versions of the index's OWN packages to add, from public signals.

The index pins year-last-12 + the top of the last 4 majors (exp101/exp152). Apps install what dependency resolution
picks: a caret range `^x.y.z` resolves to the TOP of its compatibility line (the major for >= 1.0, the 0.minor for
0.x), so an old major still depended on, or a fast-releasing 0.x package's many lines, fall between the pins
(exp167: 2.68 MB of bun-dev missed at an absent version). npm's per-version download counts say which lines are in
use. The rule, per package already in the index (never a new package):

  1. admissible = stable versions (no prerelease) published <= --as-of in the registry snapshot (the index's
     own packuments), minus every version the base index holds or refused (unpublished / no-shard);
  2. a LINE is in use when its versions' summed weekly downloads are >= --line-share of the package's
     admissible-and-indexed total AND >= --floor downloads/week -> add the line's TOP (what `^` resolves to);
  3. a single version is added on its own when its weekly downloads are >= --version-share of the total AND
     >= --floor (a version lockfiles hold although its line moved on, e.g. an exact pin).

No judged bundle, truth label or score is read: the inputs are the registry snapshot, npm's download counts, and
the base index's manifest. The thresholds are fixed by build COST (versions added), never by a score.

usage: select_versions.py <registry-dir> <base-manifest.json> <downloads-dir> <out-dir>
           [--as-of 2026-10-25T23:59:59.999Z] [--line-share 0.01] [--version-share 0.05] [--floor 200]
writes <out-dir>/delta.pins (one pkg@ver, LC_ALL=C sorted), <out-dir>/plan.json (per version: package, line,
reason, downloads, share; totals; a sizing grid over other thresholds, for the record).
"""
import json
import os
import re
import sys

SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")


def opts(argv):
    o = {"--as-of": "2026-10-25T23:59:59.999Z", "--line-share": "0.01", "--version-share": "0.05", "--floor": "200"}
    pos = []
    it = iter(argv)
    for a in it:
        if a in o:
            o[a] = next(it)
        elif a.startswith("--"):
            sys.exit(f"unknown flag {a}")
        else:
            pos.append(a)
    if len(pos) != 4:
        sys.exit(__doc__)
    return pos, o


def line_of(v):
    a, b, c = map(int, SEMVER.match(v).groups())
    return (a, b, c), (f"{a}" if a > 0 else f"0.{b}")


def packument(reg, name):
    p = os.path.join(reg, name.replace("/", "__") + ".json")
    return json.load(open(p)) if os.path.exists(p) else None


def downloads(dl, name):
    p = os.path.join(dl, name.replace("/", "__") + ".json")
    if not os.path.exists(p):
        return None
    rec = json.load(open(p))
    if rec.get("status") != 200 or not rec.get("body"):
        return None
    return rec["body"].get("downloads") or {}


def select_package(pk, held, dls, as_of, line_share, version_share, floor):
    """[(version, line, reason, downloads, share)] for one package."""
    t = pk["time"]
    stable = [v for v in pk["versions"] if SEMVER.match(v) and t.get(v, "9999") <= as_of]
    total = sum(dls.get(v, 0) for v in stable)
    if total == 0:
        return []
    lines = {}
    for v in stable:
        key, line = line_of(v)
        lines.setdefault(line, []).append((key, v))
    out = {}
    for line, members in lines.items():
        line_dl = sum(dls.get(v, 0) for _, v in members)
        top = max(members)[1]
        if top not in held and line_dl >= floor and line_dl >= line_share * total:
            out[top] = (top, line, "line-top", line_dl, line_dl / total)
        for _, v in members:
            d = dls.get(v, 0)
            if v not in held and v not in out and d >= floor and d >= version_share * total:
                out[v] = (v, line, "version", d, d / total)
    return sorted(out.values())


def main():
    (reg, manifest, dl, out), o = opts(sys.argv[1:])
    as_of = o["--as-of"]
    m = json.load(open(manifest))
    held = {}
    for p in m["packages"] + m["refused"]:
        held.setdefault(p["name"], set()).add(p["version"])
    names = sorted({p["name"] for p in m["packages"]})
    grid_params = [(ls, vs, f) for ls in (0.005, 0.01, 0.02, 0.05) for vs in (0.02, 0.05, 0.1) for f in (50, 200, 1000)]
    grid = {g: 0 for g in grid_params}
    rows, missing_dl = [], 0
    for name in names:
        pk = packument(reg, name)
        dls = downloads(dl, name)
        if pk is None:
            continue
        if dls is None:
            missing_dl += 1
            continue
        h = held.get(name, set())
        for g in grid_params:
            grid[g] += len(select_package(pk, h, dls, as_of, *g))
        for v, line, reason, d, share in select_package(
            pk, h, dls, as_of, float(o["--line-share"]), float(o["--version-share"]), float(o["--floor"])
        ):
            rows.append({"package": name, "version": v, "line": line, "reason": reason, "downloads": d,
                         "share": round(share, 6), "published": pk["time"][v]})
    os.makedirs(out, exist_ok=True)
    pins = sorted(f"{r['package']}@{r['version']}" for r in rows)
    with open(os.path.join(out, "delta.pins"), "w") as f:
        f.write("".join(p + "\n" for p in sorted(pins, key=lambda s: s.encode())))
    by_reason = {}
    for r in rows:
        by_reason[r["reason"]] = by_reason.get(r["reason"], 0) + 1
    plan = {
        "schema": "exp177-installed-versions/1",
        "asOf": as_of,
        "params": {k.lstrip("-"): o[k] for k in ("--line-share", "--version-share", "--floor")},
        "baseIndexPackages": len(names),
        "packagesWithoutDownloads": missing_dl,
        "added": len(rows),
        "addedPackages": len({r["package"] for r in rows}),
        "byReason": by_reason,
        "sizingGrid": [{"lineShare": g[0], "versionShare": g[1], "floor": g[2], "added": n} for g, n in grid.items()],
        "versions": rows,
    }
    with open(os.path.join(out, "plan.json"), "w") as f:
        json.dump(plan, f, indent=1, sort_keys=True)
    print(json.dumps({k: plan[k] for k in ("added", "addedPackages", "byReason", "packagesWithoutDownloads")}))


if __name__ == "__main__":
    main()
