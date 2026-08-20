#!/usr/bin/env python3
"""Extract (oldName -> newName) substitution pairs from name-only changed
lines between two trees, with line counts.

Method: per file, count masked-identical line texts on both sides; for lines
whose masked form matches but raw differs, align tokens positionally and
collect differing identifier pairs.

WRONGLY includes: token pairs from lines where TWO different renames happened
(positional pairing still correct per token); one-char loop vars (kept,
filterable); lines duplicated in a file are paired by multiset (fine).
Misses: renames on lines that ALSO changed structurally (charged to real).
"""
import os, re, sys, collections

KW = set("""var let const function return if else for while do switch case break continue
new typeof instanceof in of delete void null true false this async await yield throw try
catch finally class extends super import export from default require module exports
static get set""".split())
TOKEN = re.compile(r'(\.\s*)?([A-Za-z_$][A-Za-z0-9_$]*)(\s*:)?')

def mask_and_names(line):
    names = []
    def rep(m):
        dot, name, colon = m.group(1), m.group(2), m.group(3)
        if dot or colon or name in KW:
            return m.group(0)
        names.append(name)
        return (dot or '') + 'X' + (colon or '')
    return TOKEN.sub(rep, line).strip(), names

def walk(root):
    for dp, _, fns in os.walk(root):
        for fn in fns:
            if fn.endswith(".js"):
                yield os.path.relpath(os.path.join(dp, fn), root)

prior_root, fresh_root = sys.argv[1], sys.argv[2]
pairs = collections.Counter()          # (old,new) -> changed-line count
examples = {}
for rel in set(walk(prior_root)) & set(walk(fresh_root)):
    pl = open(os.path.join(prior_root, rel), encoding='utf8', errors='ignore').read().splitlines()
    fl = open(os.path.join(fresh_root, rel), encoding='utf8', errors='ignore').read().splitlines()
    pset = collections.Counter(pl); fset = collections.Counter(fl)
    gone = list((pset - fset).elements())
    came = list((fset - pset).elements())
    by_mask = collections.defaultdict(lambda: ([], []))
    for l in gone:
        m, names = mask_and_names(l)
        by_mask[m][0].append(names)
    for l in came:
        m, names = mask_and_names(l)
        by_mask[m][1].append(names)
    for m, (gs, cs) in by_mask.items():
        for gnames, cnames in zip(sorted(gs), sorted(cs)):
            if len(gnames) != len(cnames): continue
            diffs = [(a, b) for a, b in zip(gnames, cnames) if a != b]
            for a, b in diffs:
                pairs[(a, b)] += 1
                examples.setdefault((a, b), rel)

total_lines = sum(pairs.values())
print(f"substitution occurrences (changed-line-sides x pair): {total_lines}")
print(f"distinct (old->new) pairs: {len(pairs)}")
ranked = pairs.most_common()
top = sum(n for _, n in ranked[:100])
print(f"top 100 pairs hold {top} ({100*top/max(1,total_lines):.0f}%)")
import json
out = [{"old": a, "new": b, "n": n, "file": examples[(a,b)]} for (a, b), n in ranked]
json.dump(out, open(sys.argv[3], "w"), indent=0)
print("first 20:")
for (a, b), n in ranked[:20]:
    print(f"  {n:>4}  {a} -> {b}   ({examples[(a,b)]})")
