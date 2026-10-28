#!/usr/bin/env python3
"""exp159 feasibility census: build the sample pin files.

Sample: the first 100 pinned packages of exp151's npm-high-impact list
(generic popular packages, one version each — their latest at as-of
2026-10-15), plus, per package, its previous MAJOR version from the same
registry snapshot (published by the as-of, no prerelease) — the
cross-version half of the census. No app source is committed: this reads
the registry snapshot's publish times only.

Usage: select_sample.py <registry-dir> <exp151-pins.txt> <out-dir>
"""
import json
import os
import re
import sys


def main() -> None:
    reg, pins_path, out = sys.argv[1], sys.argv[2], sys.argv[3]
    asof = "2026-10-15T23:59:59.999Z"
    pins = [l.strip() for l in open(pins_path) if l.strip()]
    latest, prev, minor_specs = [], [], []
    for spec in pins[:100]:
        name, _, ver = spec.partition("@")
        if not ver:
            continue
        path = os.path.join(reg, name.replace("/", "__") + ".json")
        if not os.path.exists(path):
            print("NO PACKUMENT", name)
            continue
        times = json.load(open(path)).get("time", {})
        if not times.get(ver) or times[ver] > asof:
            continue
        latest.append(spec)
        major = int(ver.split(".")[0])
        want = f"{major - 1}."
        cands = [
            (v, t)
            for v, t in times.items()
            if v not in ("modified", "created")
            and v.startswith(want)
            and not re.search(r"[-+]", v)
            and re.match(r"^\d+\.\d+\.\d+$", v)
            and t <= asof
        ]
        if cands:
            # the newest previous-major by numeric precedence
            v = max(cands, key=lambda c: tuple(int(x) for x in c[0].split(".")))[0]
            prev.append(f"{name}@{v}")
        # and the previous MINOR of the latest (same major), for the softer
        # boundary
        minor = int(ver.split(".")[1])
        wantm = f"{major}.{minor - 1}."
        m = [
            (v, t)
            for v, t in times.items()
            if v not in ("modified", "created")
            and v.startswith(wantm)
            and not re.search(r"[-+]", v)
            and re.match(r"^\d+\.\d+\.\d+$", v)
            and t <= asof
        ]
        if m:
            vm = max(m, key=lambda c: tuple(int(x) for x in c[0].split(".")))[0]
            minor_specs.append(f"{name}@{vm}")
    os.makedirs(out, exist_ok=True)
    open(os.path.join(out, "pins-census.txt"), "w").write("\n".join(latest) + "\n")
    open(os.path.join(out, "pins-prev.txt"), "w").write("\n".join(prev) + "\n")
    allp = sorted(set(latest) | set(prev) | set(minor_specs))
    open(os.path.join(out, "pins-all.txt"), "w").write("\n".join(allp) + "\n")
    open(os.path.join(out, "pins-minor.txt"), "w").write("\n".join(minor_specs) + "\n")
    print(f"latest {len(latest)} previous-major {len(prev)} previous-minor {len(minor_specs)} all {len(allp)}")


if __name__ == "__main__":
    main()
