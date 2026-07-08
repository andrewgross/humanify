#!/usr/bin/env python3
"""Attribute rename-noise hunks to oversized (>cap) functions by line range.

Joins the cross-version diff (diff cc-A/runtime.js cc-B/runtime.js) against
the per-function line ranges emitted by truncation-coverage.ts --json. A
noise hunk (change hunk, sides structurally identical after blanking
identifiers — same rule as attribute-noise.py) is attributed to an oversized
function when its LEFT-side start line falls inside the function's input
line range (locs survive rename-only output, so output lines match).

This sizes the megafunction-addressable share of the noise BEFORE the fix,
and verifies it collapsed AFTER.

Usage:
  python3 attribute-to-megafunctions.py <runtime-diff.txt> <left-truncation.json> [top_n]
"""
import collections
import json
import re
import sys

IDENT = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*")
HUNK = re.compile(r"^(\d+(?:,\d+)?)([acd])(\d+(?:,\d+)?)$")


def norm(line: str) -> str:
    return IDENT.sub("#", line[2:])


def is_minified(name: str) -> bool:
    if "$" in name:
        return True
    if len(name) <= 3:
        return True
    if len(name) <= 4 and not re.search(r"[a-z]{3}", name):
        return True
    return not re.search(r"[a-z][a-z][a-z]", name)


def bucket(old: str, new: str) -> str:
    om, nm = is_minified(old), is_minified(new)
    if om and nm:
        return "minifier-reroll"
    if om or nm:
        return "asymmetric"
    return "transfer-gap"


def main(diff_path: str, json_path: str, top_n: int = 15) -> None:
    cov = json.load(open(json_path))
    ranges = []  # (start, end, id)
    for f in cov["oversized"]:
        if f["locStartLine"] is not None and f["locEndLine"] is not None:
            ranges.append((f["locStartLine"], f["locEndLine"], f["id"]))
    ranges.sort()

    def owner(line_no: int):
        # smallest enclosing range wins (nested oversized functions)
        best = None
        for start, end, fid in ranges:
            if start > line_no:
                break
            if end >= line_no:
                if best is None or (end - start) < (best[1] - best[0]):
                    best = (start, end, fid)
        return best[2] if best else None

    op = None
    left_start = 0
    left: list[str] = []
    right: list[str] = []

    total = {"hunks": 0, "occ": 0}
    inside = {"hunks": 0, "occ": 0}
    per_bucket_total = collections.Counter()
    per_bucket_inside = collections.Counter()
    per_fn = collections.Counter()
    per_fn_pairs = collections.defaultdict(collections.Counter)

    def flush():
        if op != "c" or len(left) != len(right):
            return
        if any(norm(a) != norm(b) for a, b in zip(left, right)):
            return
        total["hunks"] += 1
        fid = owner(left_start)
        if fid:
            inside["hunks"] += 1
        for a, b in zip(left, right):
            la, lb = IDENT.findall(a[2:]), IDENT.findall(b[2:])
            if len(la) != len(lb):
                continue
            for old, new in zip(la, lb):
                if old == new:
                    continue
                bk = bucket(old, new)
                total["occ"] += 1
                per_bucket_total[bk] += 1
                if fid:
                    inside["occ"] += 1
                    per_bucket_inside[bk] += 1
                    per_fn[fid] += 1
                    per_fn_pairs[fid][(old, new)] += 1

    with open(diff_path, encoding="utf-8", errors="replace") as fh:
        for raw in fh:
            line = raw.rstrip("\n")
            m = HUNK.match(line)
            if m:
                flush()
                op = m.group(2)
                left_start = int(m.group(1).split(",")[0])
                left, right = [], []
            elif line.startswith("< "):
                left.append(line)
            elif line.startswith("> "):
                right.append(line)
        flush()

    pct = lambda a, b: f"{100 * a / max(1, b):.1f}%"
    print(f"=== noise attribution to oversized functions ({json_path}) ===")
    print(f"noise hunks total:        {total['hunks']:,}")
    print(
        f"  inside oversized fns:   {inside['hunks']:,} ({pct(inside['hunks'], total['hunks'])})"
    )
    print(f"rename occurrences total: {total['occ']:,}")
    print(
        f"  inside oversized fns:   {inside['occ']:,} ({pct(inside['occ'], total['occ'])})"
    )
    print("\nby bucket (occurrences, inside/total):")
    for bk in ("transfer-gap", "asymmetric", "minifier-reroll"):
        print(
            f"  {bk:16} {per_bucket_inside[bk]:6,} / {per_bucket_total[bk]:6,}"
            f"  ({pct(per_bucket_inside[bk], per_bucket_total[bk])})"
        )
    print(f"\ntop {top_n} oversized functions by noise occurrences:")
    for fid, n in per_fn.most_common(top_n):
        pairs = "  ".join(
            f"{o}->{w}({c})" for (o, w), c in per_fn_pairs[fid].most_common(3)
        )
        print(f"  {fid:28} {n:6,}   {pairs}")


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(1)
    main(
        sys.argv[1], sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 15
    )
