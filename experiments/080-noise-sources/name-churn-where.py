#!/usr/bin/env python3
"""
080 — WHERE the name-only churn is, so a lever can be aimed rather than guessed.

  python3 experiments/080-noise-sources/name-churn-where.py <priorSrc> <freshSrc>

Buckets each file's name-only lines by a property of the file, then reports
concentration. A lever that reaches 5% of the mass is not worth building; the
point of this is to find out before writing code, not after a walk.
"""
import os, re, sys, collections

KW = set("""var let const function return if else for while do switch case break continue
new typeof instanceof in of delete void null true false this async await yield throw try
catch finally class extends super import export from default require module exports
static get set""".split())
TOKEN = re.compile(r'(\.\s*)?([A-Za-z_$][A-Za-z0-9_$]*)(\s*:)?')

def mask(line):
    def rep(m):
        dot, name, colon = m.group(1), m.group(2), m.group(3)
        if dot or colon or name in KW: return m.group(0)
        return (dot or '') + 'X' + (colon or '')
    return TOKEN.sub(rep, line).strip()

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
per_file = {}
total = 0
for rel in set(walk(prior_root)) & set(walk(fresh_root)):
    p = lines_of(os.path.join(prior_root, rel))
    f = lines_of(os.path.join(fresh_root, rel))
    pc, fc = collections.Counter(p), collections.Counter(f)
    gone, came = list((pc - fc).elements()), list((fc - pc).elements())
    if not gone or not came: continue
    m = collections.Counter(mask(l) for l in gone if mask(l))
    n = 0
    for l in came:
        k = mask(l)
        if m.get(k, 0) > 0:
            m[k] -= 1; n += 2
    if n:
        per_file[rel] = (n, len(f))
        total += n

print(f"name-only git lines: {total:,} across {len(per_file):,} files\n")

# concentration
ranked = sorted(per_file.items(), key=lambda kv: -kv[1][0])
for cut in (10, 25, 50, 100):
    s = sum(v[0] for _, v in ranked[:cut])
    print(f"  top {cut:>3} files hold {s:>6,} ({100*s/max(1,total):.1f}%)")

print("\nby file SIZE (a proxy for compiler-generated megafiles):")
buckets = collections.Counter(); bl = collections.Counter()
for rel, (n, size) in per_file.items():
    b = "<200 ln" if size < 200 else "200-999" if size < 1000 else "1000-4999" if size < 5000 else ">=5000"
    buckets[b] += 1; bl[b] += n
for b in ("<200 ln", "200-999", "1000-4999", ">=5000"):
    print(f"  {b:<10} {buckets[b]:>5} files  {bl[b]:>6,} lines  ({100*bl[b]/max(1,total):.1f}%)")

print("\ntop 12 files:")
for rel, (n, size) in ranked[:12]:
    print(f"  {n:>5} lines  ({size:>6,} ln file)  {rel}")
