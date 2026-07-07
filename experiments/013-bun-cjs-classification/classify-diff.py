#!/usr/bin/env python3
"""Classify a `diff` (normal format) of two humanified bundles into
rename-noise vs genuine changes, matching the exp013 PHASE4 classifier:
a CHANGE hunk with equal line counts on both sides that becomes identical
after replacing every identifier token with '#' is pure rename-noise."""
import re
import sys

IDENT = re.compile(r"\b[A-Za-z_$][A-Za-z0-9_$]*\b")
HUNK = re.compile(r"^(\d+(?:,\d+)?)([acd])(\d+(?:,\d+)?)$")


def norm(line: str) -> str:
    # strip the leading "< " / "> " marker, then blank out identifiers
    return IDENT.sub("#", line[2:])


def main(path: str) -> None:
    noise_hunks = noise_lines = 0
    real_hunks = real_lines = 0
    add_hunks = add_lines = 0
    del_hunks = del_lines = 0
    chg_hunks = 0
    total_hunks = 0

    left: list[str] = []
    right: list[str] = []
    op = None
    in_right = False

    def flush():
        nonlocal noise_hunks, noise_lines, real_hunks, real_lines
        nonlocal add_hunks, add_lines, del_hunks, del_lines, chg_hunks
        if op is None:
            return
        if op == "a":
            add_hunks += 1
            add_lines += len(right)
            real_hunks += 1
            real_lines += len(right)
        elif op == "d":
            del_hunks += 1
            del_lines += len(left)
            real_hunks += 1
            real_lines += len(left)
        elif op == "c":
            chg_hunks += 1
            same_count = len(left) == len(right)
            normalized_equal = same_count and all(
                norm(a) == norm(b) for a, b in zip(left, right)
            )
            if normalized_equal:
                noise_hunks += 1
                noise_lines += len(left) + len(right)
            else:
                real_hunks += 1
                real_lines += len(left) + len(right)

    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.rstrip("\n")
            m = HUNK.match(line)
            if m:
                flush()
                total_hunks += 1
                op = m.group(2)
                left, right, in_right = [], [], False
                continue
            if op is None:
                continue
            if line == "---":
                in_right = True
            elif line.startswith("< "):
                left.append(line)
            elif line.startswith("> "):
                right.append(line)
        flush()

    print(f"total change hunks:      {total_hunks:>8,}")
    print(f"  add (new code):        {add_hunks:>8,}  ({add_lines:,} lines)")
    print(f"  delete (removed code): {del_hunks:>8,}  ({del_lines:,} lines)")
    print(f"  change:                {chg_hunks:>8,}")
    print(f"    -> rename-noise:     {noise_hunks:>8,}  ({noise_lines:,} lines)")
    print(
        f"    -> real change:      {chg_hunks - noise_hunks:>8,}  "
        f"({real_lines - add_lines - del_lines:,} lines)"
    )
    print("-" * 48)
    print(f"NOISE hunks:             {noise_hunks:>8,}  ({noise_lines:,} lines)")
    print(f"REAL  hunks:             {real_hunks:>8,}  ({real_lines:,} lines)")
    pct = 100 * noise_hunks / total_hunks if total_hunks else 0
    print(f"noise share of hunks:    {pct:>7.1f}%")


if __name__ == "__main__":
    main(sys.argv[1])
