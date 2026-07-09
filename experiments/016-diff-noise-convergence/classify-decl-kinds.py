#!/usr/bin/env python3
"""Classify rename-noise pairs by the OLD name's declaration kind.

Joins the cross-version diff's noise pairs against the LEFT output's
`function <name>(` and `class <name>` declarations. Splits the noise into
function-decl-name renames (the serializeWithHelper family — nothing
match-based pins a drifted function's name), class-decl-name renames
(nodeless before exp016), and everything else (locals/params/module vars).

Usage: classify-decl-kinds.py <runtime-diff.txt> <left-runtime.js> [top_n]
"""
import collections
import re
import sys

IDENT = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*")
HUNK = re.compile(r"^(\d+(?:,\d+)?)([acd])(\d+(?:,\d+)?)$")
FN_DECL = re.compile(r"\bfunction ([A-Za-z_$][A-Za-z0-9_$]*)\s*\(")
CLASS_DECL = re.compile(r"\bclass ([A-Za-z_$][A-Za-z0-9_$]*)")


def norm(line: str) -> str:
    return IDENT.sub("#", line[2:])


def main(diff_path: str, left_path: str, top_n: int = 8) -> None:
    fn_names, class_names = set(), set()
    with open(left_path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = FN_DECL.search(line)
            if m:
                fn_names.add(m.group(1))
            m = CLASS_DECL.search(line)
            if m:
                class_names.add(m.group(1))

    op = None
    left: list[str] = []
    right: list[str] = []
    per_pair = collections.Counter()

    def flush():
        if op != "c" or len(left) != len(right):
            return
        if any(norm(a) != norm(b) for a, b in zip(left, right)):
            return
        for a, b in zip(left, right):
            la, lb = IDENT.findall(a[2:]), IDENT.findall(b[2:])
            if len(la) != len(lb):
                continue
            for old, new in zip(la, lb):
                if old != new:
                    per_pair[(old, new)] += 1

    with open(diff_path, encoding="utf-8", errors="replace") as fh:
        for raw in fh:
            line = raw.rstrip("\n")
            m = HUNK.match(line)
            if m:
                flush()
                op = m.group(2)
                left, right = [], []
            elif line.startswith("< "):
                left.append(line)
            elif line.startswith("> "):
                right.append(line)
        flush()

    kind_occ = collections.Counter()
    kind_pairs = collections.Counter()
    top_by_kind = collections.defaultdict(list)
    for (old, new), n in per_pair.items():
        if old in fn_names:
            kind = "function-decl"
        elif old in class_names:
            kind = "class-decl"
        else:
            kind = "other"
        kind_occ[kind] += n
        kind_pairs[kind] += 1
        top_by_kind[kind].append((n, old, new))

    total = sum(per_pair.values())
    print(f"total rename occurrences: {total:,}")
    for kind in ("function-decl", "class-decl", "other"):
        print(f"  {kind:14} {kind_occ[kind]:6,} occ | {kind_pairs[kind]:5,} pairs")
        for n, old, new in sorted(top_by_kind[kind], reverse=True)[:top_n]:
            print(f"      {n:4} {old} -> {new}")


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(1)
    main(
        sys.argv[1],
        sys.argv[2],
        int(sys.argv[3]) if len(sys.argv) > 3 else 8
    )
