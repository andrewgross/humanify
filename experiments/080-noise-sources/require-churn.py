#!/usr/bin/env python3
"""
080 — split changed `require` lines into ALIAS churn (noise) vs PATH change (real).

  python3 experiments/080-noise-sources/require-churn.py <priorSrc> <freshSrc>

Written after mis-reading one diff line. I saw

  -const srcStripAnsi = require("../strip-ansi.js");
  +const stripAnsi2   = require("../strip-ansi-2.js");

and concluded an existing file had yielded its name to a newcomer. It had not:
2.1.216 keeps strip-ansi.js for the same module and gives strip-ansi-2.js to a
genuinely different one. The rule under suspicion was already correct.

So the question has to be asked precisely. For each file, of its changed require
lines:
  ALIAS-ONLY  same path, different local name  -> pure noise, ours to fix
  PATH        different path                   -> a real dependency change
Counting "distinct modules whose alias changed" without that split conflates
the two, which is how the 411 figure came about.
"""
import os, re, sys, collections

REQ = re.compile(r'^const\s+([A-Za-z0-9_$]+)\s*=\s*require\("([^"]+)"\);?\s*$')

def requires(path):
    out = {}
    try:
        with open(path, encoding="utf8", errors="ignore") as fh:
            for line in fh:
                m = REQ.match(line.strip())
                if m:
                    out.setdefault(m.group(2), []).append(m.group(1))
    except OSError:
        pass
    return out

def walk(root):
    for dp, _, fns in os.walk(root):
        for fn in fns:
            if fn.endswith(".js"):
                yield os.path.relpath(os.path.join(dp, fn), root)

prior_root, fresh_root = sys.argv[1], sys.argv[2]
alias_only = 0
path_changed = 0
added = 0
removed = 0
files_with_alias_churn = set()
examples = []

common = set(walk(prior_root)) & set(walk(fresh_root))
for rel in common:
    p = requires(os.path.join(prior_root, rel))
    f = requires(os.path.join(fresh_root, rel))
    for path, pnames in p.items():
        fnames = f.get(path)
        if fnames is None:
            removed += 1
            continue
        for a, b in zip(sorted(pnames), sorted(fnames)):
            if a != b:
                alias_only += 1
                files_with_alias_churn.add(rel)
                if len(examples) < 6:
                    examples.append((rel, path, a, b))
    for path in f:
        if path not in p:
            added += 1
    path_changed += 0

print(f"files compared                       {len(common):>8,}")
print(f"require lines, SAME path new ALIAS   {alias_only:>8,}   <- noise, ours")
print(f"  in files                           {len(files_with_alias_churn):>8,}")
print(f"requires removed (path gone)         {removed:>8,}   <- dependency change")
print(f"requires added   (path new)          {added:>8,}   <- dependency change")
print("\nexamples of ALIAS-ONLY churn:")
for rel, path, a, b in examples:
    print(f"  {rel}\n    {path}: {a} -> {b}")
