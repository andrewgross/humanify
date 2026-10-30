#!/usr/bin/env python3
"""exp165 census: per claimed region, how each unit got there and what it is.

Reads a `humanify discover` report (unit-prints + call-context + bundle-order,
names-mode config) and classifies:

  per REGION (a placement group = one lazy module's statements; the factory
  modules are separate) - units by claim mechanism:
    seed        the unit's own claim is a round-0 seed (index/declared/banner)
    down        DOWN propagation (every caller claimed)
    module      MODULE fill (the whole module group is one package)
    glue        GLUE (a wrapper whose body is all P)
    unclaimed   no claim (region fill by containment, or the group is partial)
  and hold status per unit vs the CLAIMED family:
    held        the family holds a print of the unit (whole or function, at
                or over the 20-node row floor)
    stranger    the family holds nothing of it, sub-typed:
      sub-floor   every print of the unit is under the row floor (can't tell)
      no-family   prints exist at/over the floor, no family holds any
      cross-claim other families hold it (cross-claim)

usage: python3 -I census.py <report.json> [--truth <truth.json>]
"""
import bisect
import collections
import json
import sys

ROW_FLOOR = 20  # libindex::units::UNIT_ROW_MIN_NODES


def load(path):
    with open(path) as fh:
        return json.load(fh)


def unit_index(report):
    """(start,end) -> unit-row index (graph order)."""
    rows = report["libraryDiscovery"]["unitRows"]
    return {(r["start"], r["end"]): i for i, r in enumerate(rows)}


def claims_by_span(ld):
    out = {}
    for c in ld["claims"]:
        out[(c["start"], c["end"])] = c
    return out


def build(report):
    ld = report["libraryDiscovery"]
    rows = ld["unitRows"]
    key = unit_index(report)
    claims = claims_by_span(ld)
    # callContext: unit -> families (family name -> covered nodes); only units
    # the index holds ANY print of appear.
    ctx = {}
    for row in ld["callContext"]:
        fams = {f["package"]: f["covered"] for f in row["families"]}
        pick = row.get("pick")
        pick_pkg = pick["package"] if pick else None
        ctx[row["unit"]] = {
            "route": row["route"],
            "whole_nodes": row["wholeNodes"],
            "owners": row["owners"],
            "families": fams,
            "pick": pick_pkg,
        }
    return ld, rows, key, claims, ctx


def hold_status(i, rows, ctx, claimed_pkg, same=None):
    """One unit's hold status vs the claimed package (family names already
    resolved by the index's own family rule; we compare family keys here)."""
    r = rows[i]
    w = r.get("whole")
    fns = r.get("functions", [])
    max_nodes = max([w["nodes"] if w else 0] + [f["nodes"] for f in fns])
    if max_nodes < ROW_FLOOR:
        return "sub-floor"
    fams = ctx.get(i, {}).get("families", {})
    if not fams:
        return "no-family"
    if claimed_pkg in fams:
        return "held"
    return "cross-claim"


def region_rows(ld):
    """The placement groups (lazy modules) as regions."""
    return ld["placementGroups"]


def classify_group(g, rows, key, claims, ctx):
    """One region's composition."""
    stats = collections.Counter()
    stranger_bytes = collections.Counter()
    units = []
    for st in g["statements"]:
        i = key.get((st["start"], st["end"]))
        if i is None:
            stats["not-a-unit"] += 1
            continue
        r = rows[i]
        b = r["bytes"]
        c = claims.get((st["start"], st["end"]))
        if c is None:
            stats["unclaimed"] += 1
            stats["unclaimed_bytes"] += b
            units.append((i, "unclaimed", None, "unclaimed", b))
            continue
        pkg = c["package"]
        rule = c["rule"]
        stats[f"rule:{rule}"] += 1
        stats[f"rule:{rule}_bytes"] += b
        hs = hold_status(i, rows, ctx, pkg)
        stats[f"hold:{hs}"] += 1
        stats[f"hold:{hs}_bytes"] += b
        if hs != "held":
            stranger_bytes[hs] += b
        units.append((i, rule, pkg, hs, b))
    # the verdict: all claimed units one package and no unclaimed row?
    pkgs = {c["package"] for c in (claims.get((st["start"], st["end"])) for st in g["statements"]) if c}
    if len(pkgs) > 1:
        verdict = "contested"
    elif not pkgs:
        verdict = "unclaimed"
    elif any((st["start"], st["end"]) not in claims for st in g["statements"]):
        verdict = "partial"
    else:
        verdict = "clean"
    return {
        "verdict": verdict,
        "stats": dict(stats),
        "stranger_bytes": dict(stranger_bytes),
        "units": units,
    }


def main():
    report = load(sys.argv[1])
    ld, rows, key, claims, ctx = build(report)
    groups = region_rows(ld)
    print(f"units {len(rows)}, claims {len(claims)}, groups {len(groups)}, "
          f"callContext {len(ld['callContext'])}")
    totals = collections.Counter()
    per_verdict = collections.Counter()
    for g in groups:
        res = classify_group(g, rows, key, claims, ctx)
        per_verdict[res["verdict"]] += 1
        for k, v in res["stats"].items():
            totals[k] += v
    print("verdicts:", dict(per_verdict))
    print("composition (all groups):")
    for k in sorted(totals):
        print(f"  {k}: {totals[k]}")


if __name__ == "__main__":
    main()
