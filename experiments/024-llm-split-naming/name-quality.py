#!/usr/bin/env python3
"""Score a split tree's file/folder NAMES (not contents).

Flags the low-quality name shapes exp024 targets: var-decorated (*Val),
placeholder (noop, reactLibNN, initializeModuleNN), minted-ish (short,
digit-laden, no real word), and generic (utils/helpers/...). Reports the
share of clean names, per tree, so mechanical vs LLM trees compare.

Usage: name-quality.py <tree1> [tree2 ...]
"""
import json
import os
import re
import sys

GENERIC = {"utils", "util", "helpers", "helper", "misc", "core", "common",
           "lib", "libs", "main", "index", "shared", "module", "modules",
           "code", "src", "functions", "handlers", "types"}


def stem(path):
    return os.path.basename(path).removesuffix(".js")


def classify(name):
    low = name.lower()
    if low in GENERIC:
        return "generic"
    if re.search(r"Val\d*$", name) or re.match(r"^(noop|placeholder)", low):
        return "decorated"
    if re.match(r"^(reactLib|initializeModule|lib_)\d*", name) or re.match(r"^lib[A-Z0-9]", name):
        return "placeholder"
    # minted-ish: short, digit-heavy, or no 3-letter run
    core = re.sub(r"[_$]", "", name)
    if len(core) <= 3 or not re.search(r"[a-z]{3}", name):
        return "minted"
    return "clean"


def score_tree(tree):
    ledger = json.load(open(os.path.join(tree, "_split-ledger.json")))
    files = ledger["files"]
    folders = sorted(set(f.split("/")[0] for f in files))
    file_stems = [stem(f) for f in files]

    def tally(names):
        c = {}
        for n in names:
            c[classify(n)] = c.get(classify(n), 0) + 1
        return c

    return folders, file_stems, tally(folders), tally(file_stems)


for tree in sys.argv[1:]:
    folders, file_stems, ft, filt = score_tree(tree)
    print(f"\n=== {tree} ===")
    for label, tally, total in (("folders", ft, len(folders)),
                                 ("files", filt, len(file_stems))):
        clean = tally.get("clean", 0)
        bad = total - clean
        print(f"  {label}: {clean}/{total} clean ({100*clean/total:.0f}%)  "
              f"bad={ {k: v for k, v in tally.items() if k != 'clean'} }")
    print(f"  folder names: {', '.join(folders[:12])}"
          + (" ..." if len(folders) > 12 else ""))
