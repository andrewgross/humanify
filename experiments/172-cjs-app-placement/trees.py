#!/usr/bin/env python3
"""exp172 phase 2: the output tree of each named bundle, before and after,
as a summary — files and bytes under src/, vendor/ (the manifest
excluded) and anything else (runtime.js, the run scaffold) — from the
tree.tsv files bundev-runs.sh writes (`<path>\\t<size>` per file).

usage: python3 -I trees.py <before-runs-dir> <after-runs-dir> <bundle-substring>...
"""
import os
import sys


def summary(path):
    if not os.path.exists(path):
        return None
    out = {"src": [0, 0], "vendor": [0, 0], "other": [0, 0]}
    for line in open(path):
        rel, size = line.rstrip("\n").rsplit("\t", 1)
        if rel == "vendor/_bun-modules.json":
            continue
        side = rel.split("/", 1)[0] if "/" in rel else "other"
        side = side if side in out else "other"
        out[side][0] += 1
        out[side][1] += int(size)
    return out


def cell(s, side):
    return "—" if s is None else f"{s[side][0]} / {s[side][1]:,}"


def main():
    before, after, wanted = sys.argv[1], sys.argv[2], sys.argv[3:]
    ids = sorted(i for i in os.listdir(after) if os.path.isdir(os.path.join(after, i))
                 and any(w in i for w in wanted))
    print("| bundle | src/ before (files / bytes) | src/ after | vendor/ before | vendor/ after |"
          " other before | other after |")
    print("| --- | --- | --- | --- | --- | --- | --- |")
    for i in ids:
        b = summary(os.path.join(before, i, "tree.tsv"))
        a = summary(os.path.join(after, i, "tree.tsv"))
        print(f"| {i.replace('bun-dev-', '')} | {cell(b, 'src')} | {cell(a, 'src')} | "
              f"{cell(b, 'vendor')} | {cell(a, 'vendor')} | {cell(b, 'other')} | {cell(a, 'other')} |")


if __name__ == "__main__":
    main()
