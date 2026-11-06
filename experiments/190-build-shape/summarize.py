#!/usr/bin/env python3
"""exp190 (b): roll the per-package comparisons (compare.py) up by size class
and by module format, for every direct route. Usage:
  summarize.py <routes-root> <cmp-dir>   (reads <cmp-dir>/cmp-<route>.json)"""
import json
import os
import sys

ROOT, CMP = sys.argv[1:3]
ROUTES = ("nobundle", "single", "single-esm", "pkg-bundle")


def safe(n):
    return n.replace("/", "__")


def strings(v):
    if isinstance(v, str):
        yield v
    elif isinstance(v, dict):
        for x in v.values():
            yield from strings(x)
    elif isinstance(v, list):
        for x in v:
            yield from strings(x)


def conditions(v):
    if isinstance(v, dict):
        for k, x in v.items():
            yield k
            yield from conditions(x)
    elif isinstance(v, list):
        for x in v:
            yield from conditions(x)


def fmt(name, version):
    p = os.path.join(ROOT, f"{safe(name)}@{version}", "direct-single-esm", "package", "package.json")
    m = json.load(open(p))
    conds = set(conditions(m.get("exports")))
    if m.get("type") == "module":
        f = "esm"
    elif "import" in conds and "require" in conds or m.get("module"):
        f = "dual"
    else:
        f = "cjs"
    targets = [m.get("main") or "", m.get("module") or ""] + list(strings(m.get("exports")))
    dist = any(t.lstrip("./").split("/")[0] in ("dist", "build", "out", "cjs", "esm") for t in targets if t)
    return f, dist


def roll(rows):
    keys = ("units", "nodes", "hit", "hitNodes", "hitAny", "hitAnyNodes")
    t = {k: sum(r[k] for r in rows) for k in keys}
    t["n"] = len(rows)
    t["bundleSec"] = sum(r["time"]["bundle"] or 0 for r in rows)
    t["directSec"] = sum(r["time"]["direct"] or 0 for r in rows)
    miss = {}
    for r in rows:
        for c, n in r["missAny"].items():
            miss[c] = miss.get(c, 0) + n
    t["missAny"] = miss
    t["artifacts"] = sum(sum(r["artifacts"].values()) for r in rows)
    t["allUnits"] = sum(r["allUnits"] for r in rows)
    return t


def pct(a, b):
    return f"{100 * a / b:5.1f}%" if b else "  -  "


def main():
    for route in ROUTES:
        path = os.path.join(CMP, f"cmp-{route}.json")
        if not os.path.exists(path):
            continue
        rows = json.load(open(path))
        for r in rows:
            r["format"], r["dist"] = fmt(r["name"], r["version"])
        groups = {"ALL": rows}
        for c in ("tiny", "medium", "large"):
            groups[c] = [r for r in rows if r["class"] == c]
        for f in ("esm", "dual", "cjs"):
            groups[f"fmt:{f}"] = [r for r in rows if r["format"] == f]
        groups["prebuilt-dist"] = [r for r in rows if r["dist"]]
        groups["source-layout"] = [r for r in rows if not r["dist"]]
        print(f"\n### route {route}")
        print("| group | pkgs | shard rows (all) | artifact rows | package rows | same hash anywhere | by nodes | same file | bundle s | direct s |")
        print("|---|---|---|---|---|---|---|---|---|---|")
        for g, rs in groups.items():
            t = roll(rs)
            print(
                f"| {g} | {t['n']} | {t['allUnits']} | {t['artifacts']} | {t['units']} | {t['hitAny']} ({pct(t['hitAny'], t['units'])}) "
                f"| {pct(t['hitAnyNodes'], t['nodes'])} | {pct(t['hit'], t['units'])} | {t['bundleSec']:.0f} | {t['directSec']:.0f} |"
            )
        t = roll(rows)
        print("misses (package-wide):", json.dumps(dict(sorted(t["missAny"].items(), key=lambda kv: -kv[1]))))


if __name__ == "__main__":
    main()
