#!/usr/bin/env python3
"""
080 — byte-identical lines that still cost diff lines because they MOVED.

  python3 experiments/080-noise-sources/moved-lines.py <priorSrc> <freshSrc>

A first version of this counted 4,320 lines as "moved between files" by pairing
identical deleted and added lines positionally. That is wrong in an obvious way
once stated: `}`, `});`, `return;` and friends appear in thousands of files, so
any deletion of one and addition of another anywhere counts as a move. This
requires a line to be SUBSTANTIAL before it can be evidence of movement, and
reports the trivial share so the filter's effect is visible rather than assumed.
"""
import os, re, sys, collections

TRIVIAL = re.compile(r'^[\s{}()\[\];,]*$|^(return|break|continue|else|\}\s*else\s*\{)[\s;{}]*$')
MIN_LEN = 25

def walk(root):
    for dp, _, fns in os.walk(root):
        for fn in fns:
            if fn.endswith(".js"):
                yield os.path.relpath(os.path.join(dp, fn), root)

def lines_of(p):
    try:
        with open(p, encoding="utf8", errors="ignore") as fh:
            return [l.rstrip("\n") for l in fh]
    except OSError:
        return []

prior_root, fresh_root = sys.argv[1], sys.argv[2]
gone = collections.defaultdict(list)   # text -> [file]
came = collections.defaultdict(list)
trivial_gone = trivial_came = 0

files = set(walk(prior_root)) | set(walk(fresh_root))
for rel in files:
    p = collections.Counter(lines_of(os.path.join(prior_root, rel)))
    f = collections.Counter(lines_of(os.path.join(fresh_root, rel)))
    for text, n in (p - f).items():
        if TRIVIAL.match(text.strip()) or len(text.strip()) < MIN_LEN:
            trivial_gone += n; continue
        gone[text].extend([rel] * n)
    for text, n in (f - p).items():
        if TRIVIAL.match(text.strip()) or len(text.strip()) < MIN_LEN:
            trivial_came += n; continue
        came[text].extend([rel] * n)

same_file = cross_file = 0
for text, dfiles in gone.items():
    afiles = came.get(text)
    if not afiles:
        continue
    df = collections.Counter(dfiles); af = collections.Counter(afiles)
    for rel, n in df.items():
        m = min(n, af.get(rel, 0))
        same_file += m
    paired = min(len(dfiles), len(afiles))
    cross_file += paired - sum(min(n, af.get(rel, 0)) for rel, n in df.items())

print(f"SUBSTANTIAL identical lines that moved (>= {MIN_LEN} chars, non-trivial):")
print(f"  between files   {cross_file*2:>8,} git lines")
print(f"  within a file   {same_file*2:>8,} git lines")
print(f"\ntrivial/short lines excluded: {trivial_gone:,} deleted, {trivial_came:,} added")
print("A first pass counted those as movement, which inflated the figure.")
