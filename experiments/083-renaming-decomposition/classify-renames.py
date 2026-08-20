#!/usr/bin/env python3
"""Classify the (old->new) rename pairs by mechanism.

Buckets (checked in order):
  alias-form-flip : same stem modulo 'src' prefix / trailing digits — the
                    import alias re-derived in a different collision state
  path-derived    : old resolves to a PRIOR file path's camelization and new
                    to a FRESH file path's camelization (with the same
                    prefix/digit tolerance) — alias following a module whose
                    path or identity changed (moves, -2 mints, regrouping)
  word-choice     : neither — the model picked a different word

WRONGLY includes (stated per measurement rules): tiny modules named after
their single export make export renames look 'path-derived' when the FILE was
named after the fresh export (the file follows the name, not the reverse).
Sampled below to check.
"""
import json, os, re, sys, collections

pairs = json.load(open(sys.argv[1]))
prior_root, fresh_root = sys.argv[2], sys.argv[3]

def camel(path):
    stem = os.path.basename(path)[:-3]
    parts = re.split(r'[-_]', stem)
    return parts[0] + "".join(p.capitalize() for p in parts[1:])

def campaths(root):
    out = collections.defaultdict(set)
    for dp, _, fns in os.walk(root):
        for fn in fns:
            if fn.endswith(".js"):
                rel = os.path.relpath(os.path.join(dp, fn), root)
                out[camel(rel).lower()].add(rel)
                # full-path camelization too: parse-command-flags/analyze-features
                full = re.split(r'[-_/]', rel[:-3])
                fullc = full[0] + "".join(p.capitalize() for p in full[1:])
                out[fullc.lower()].add(rel)
    return out

P = campaths(prior_root)
F = campaths(fresh_root)

def norm(name):
    n = name
    n = re.sub(r'^src', '', n)
    n = re.sub(r'\d+$', '', n)
    return (n[0].lower() + n[1:]) if n else n

def pathkey(name):
    return norm(name).lower()

buckets = collections.Counter()
mass = collections.Counter()
samples = collections.defaultdict(list)
for e in pairs:
    o, n, cnt = e["old"], e["new"], e["n"]
    if norm(o) == norm(n):
        b = "alias-form-flip"
    elif pathkey(o) in P and pathkey(n) in F:
        b = "path-derived"
    else:
        b = "word-choice"
    buckets[b] += 1
    mass[b] += cnt
    if len(samples[b]) < 12:
        samples[b].append(f"{cnt:>3} {o} -> {n}")

total = sum(mass.values())
print(f"total substitution line-pairs: {total} (~{total*2} git lines)")
for b in ("alias-form-flip", "path-derived", "word-choice"):
    print(f"  {b:<16} {mass[b]:>5} line-pairs ({100*mass[b]/total:.0f}%)  across {buckets[b]} name pairs")
for b in ("alias-form-flip", "path-derived", "word-choice"):
    print(f"\n== {b} samples:")
    for s in samples[b]:
        print("   ", s)
