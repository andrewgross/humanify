"""exp101: the versions a TAIL root is indexed at, emitted as humanify-index `--pinned` specs.

usage: python3 -I tail_pins.py <registry dir> <as-of YYYY-MM-DD> <policy> <roots.txt> <out pins.txt>
policy:
  year-all+major      every stable release published in the 12 months before as-of, plus the newest stable
                      release of every older major (0.x: of every older 0.minor — a caret range on 0.y pins
                      the minor). What a caret-range install resolves to at any time in the last year, plus
                      today's resolution of every older caret range.
  year-last<N>+major  the same, but at most the N newest releases of the 12-month window (high-churn release
                      trains: nightly-ish SDKs publish 100+ releases a year).
  year-last<N>+major<M>  and at most the M newest older release LINES (a major, or a 0.minor): a 0.x package
                      that cuts a minor per release (lucide-react: 564 lines) would otherwise index every one.
Stable = plain x.y.z with a publish time, published on or before as-of, installable (in `versions`).
Mirrors humanify-index's own `year-all+minor` reading of the packument (asof.rs), so a tail root costs what the
builder's policy would cost minus the older minors.
"""
import datetime
import json
import os
import re
import sys

SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")


def stable(doc, as_of):
    out = []
    times = doc.get("time", {})
    for v in doc.get("versions", []):
        m = SEMVER.match(v)
        t = times.get(v)
        if not m or not t:
            continue
        t = t.replace("Z", "+00:00")
        if datetime.datetime.fromisoformat(t) > as_of:
            continue
        out.append(((int(m[1]), int(m[2]), int(m[3])), v, datetime.datetime.fromisoformat(t)))
    out.sort(reverse=True)
    return out


def pick(doc, as_of, policy):
    vs = stable(doc, as_of)
    since = as_of - datetime.timedelta(days=365)
    cap = lines_cap = None
    m = re.match(r"^year-last(\d+)\+major(\d*)$", policy)
    if m:
        cap = int(m[1])
        lines_cap = int(m[2]) if m[2] else None
    elif policy != "year-all+major":
        raise SystemExit(f"unknown policy {policy}")
    recent = [v for s, v, t in vs if t > since]
    if cap is not None:
        recent = recent[:cap]
    lines = set()
    keep = set(recent)
    for s, v, t in vs:
        line = (s[0], s[1]) if s[0] == 0 else (s[0],)
        if line not in lines:
            lines.add(line)
            if lines_cap is None or len(lines) <= lines_cap:
                keep.add(v)
    return [v for _, v, _ in vs if v in keep]


def main():
    reg, as_of, policy, roots, out = sys.argv[1:6]
    as_of = datetime.datetime.fromisoformat(as_of + "T23:59:59.999+00:00")
    specs = []
    for name in (l.strip() for l in open(roots)):
        if not name:
            continue
        p = os.path.join(reg, name.replace("/", "__") + ".json")
        if not os.path.exists(p):
            continue
        specs += [f"{name}@{v}" for v in pick(json.load(open(p)), as_of, policy)]
    open(out, "w").write("\n".join(specs) + "\n")
    print(f"{len(specs)} pins for {policy}")


if __name__ == "__main__":
    main()
