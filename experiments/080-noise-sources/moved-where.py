#!/usr/bin/env python3
"""
080 — WHERE does unchanged code move to? Attribution for the cross-file moves.

  python3 experiments/080-noise-sources/moved-where.py <priorSrc> <freshSrc>

2,416 git lines are byte-identical and still cost a delete plus an add because
the line landed in a different file. Before proposing anything, find out whether
those moves are concentrated in a few file PAIRS (a module that got re-split, a
fixable placement decision) or scattered one-off (inherent to real change).
"""
import os, sys, collections, re

TRIVIAL = re.compile(r'^[\s{}()\[\];,]*$|^(return|break|continue|else)[\s;{}]*$')
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
gone = collections.defaultdict(list)
came = collections.defaultdict(list)
for rel in set(walk(prior_root)) | set(walk(fresh_root)):
    p = collections.Counter(lines_of(os.path.join(prior_root, rel)))
    f = collections.Counter(lines_of(os.path.join(fresh_root, rel)))
    for text, n in (p - f).items():
        t = text.strip()
        if TRIVIAL.match(t) or len(t) < MIN_LEN: continue
        gone[text].extend([rel] * n)
    for text, n in (f - p).items():
        t = text.strip()
        if TRIVIAL.match(t) or len(t) < MIN_LEN: continue
        came[text].extend([rel] * n)

pairs = collections.Counter()
for text, dfiles in gone.items():
    afiles = came.get(text)
    if not afiles: continue
    df = collections.Counter(dfiles); af = collections.Counter(afiles)
    for src, n in df.items():
        left = n - min(n, af.get(src, 0))     # same-file copies are not moves
        if left <= 0: continue
        for dst, m in af.items():
            if dst == src: continue
            take = min(left, m)
            if take <= 0: continue
            pairs[(src, dst)] += take
            left -= take
            if left == 0: break

total = sum(pairs.values())
print(f"cross-file moved lines: {total*2:,} git lines over {len(pairs):,} file PAIRS\n")
ranked = pairs.most_common()
for cut in (10, 25, 50):
    s = sum(n for _, n in ranked[:cut])
    print(f"  top {cut:>3} pairs hold {s*2:>6,} ({100*s/max(1,total):.1f}%)")

print("\nsame-FOLDER moves vs across folders:")
same_dir = sum(n for (a, b), n in pairs.items() if os.path.dirname(a) == os.path.dirname(b))
print(f"  same folder  {same_dir*2:>6,} ({100*same_dir/max(1,total):.1f}%)")
print(f"  different    {(total-same_dir)*2:>6,} ({100*(total-same_dir)/max(1,total):.1f}%)")

print("\ntop 10 file pairs:")
for (a, b), n in ranked[:10]:
    print(f"  {n*2:>5} ln  {a}\n            -> {b}")
