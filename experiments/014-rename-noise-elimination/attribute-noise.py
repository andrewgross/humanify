#!/usr/bin/env python3
"""Attribute cross-version rename-noise to its root cause.

Input: a normal `diff cc-A/runtime.js cc-B/runtime.js` (the cross-version
diff of two humanified bundles). A "rename-noise" hunk is a CHANGE hunk whose
two sides are identical after blanking identifiers — i.e. structurally the
same code with different names (see classify-diff.py). Since the two sides are
structurally identical, their identifier token streams line up 1:1, so we can
recover every (old -> new) rename that a noise hunk encodes.

We then bucket each rename by what it tells us about WHY the name diverged:

  transfer-gap       both names are descriptive but different — the LLM named
                     the same binding two different ways across the runs; a
                     working cross-version transfer would have reused one.
  asymmetric         one side is descriptive, the other minified-looking — one
                     run named the binding, the other left it minified (a
                     matching / transfer asymmetry, or a naming failure).
  minifier-reroll    both sides minified-looking — neither run named it and Bun
                     re-minted the token between builds (needs a deterministic
                     naming floor, not a transfer).

Occurrences ~= diff lines a rename contributes; distinct (old,new) pairs ~=
distinct bindings. Ranking pairs by occurrence points at the bindings whose
transfer would remove the most noise.

Usage: python3 attribute-noise.py <runtime-diff.txt> [top_n]
"""
import collections
import re
import sys

IDENT = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*")
HUNK = re.compile(r"^(\d+(?:,\d+)?)([acd])(\d+(?:,\d+)?)$")


def norm(line: str) -> str:
    return IDENT.sub("#", line[2:])


def is_minified(name: str) -> bool:
    """Heuristic: Bun minified survivor vs an LLM-given descriptive name."""
    if "$" in name:
        return True
    if len(name) <= 3:
        return True
    # short mixed-case / trailing-underscore tokens like wP_, qHd, Glq
    if len(name) <= 4 and not re.search(r"[a-z]{3}", name):
        return True
    # a descriptive name has a run of lowercase letters (a word)
    return not re.search(r"[a-z][a-z][a-z]", name)


def bucket(old: str, new: str) -> str:
    om, nm = is_minified(old), is_minified(new)
    if om and nm:
        return "minifier-reroll"
    if om or nm:
        return "asymmetric"
    return "transfer-gap"


def main(path: str, top_n: int = 30) -> None:
    op = None
    left: list[str] = []
    right: list[str] = []
    noise_hunks = 0
    per_pair = collections.Counter()  # (old,new) -> occurrences
    per_bucket_occ = collections.Counter()
    per_bucket_pairs: dict[str, set] = collections.defaultdict(set)

    def flush():
        nonlocal noise_hunks
        if op != "c" or len(left) != len(right):
            return
        if any(norm(a) != norm(b) for a, b in zip(left, right)):
            return  # not rename-noise (a real change)
        noise_hunks += 1
        for a, b in zip(left, right):
            la, lb = IDENT.findall(a[2:]), IDENT.findall(b[2:])
            if len(la) != len(lb):
                continue
            for oa, ob in zip(la, lb):
                if oa != ob:
                    per_pair[(oa, ob)] += 1
                    bkt = bucket(oa, ob)
                    per_bucket_occ[bkt] += 1
                    per_bucket_pairs[bkt].add((oa, ob))

    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.rstrip("\n")
            m = HUNK.match(line)
            if m:
                flush()
                op, left, right = m.group(2), [], []
            elif op and line.startswith("< "):
                left.append(line)
            elif op and line.startswith("> "):
                right.append(line)
        flush()

    total_occ = sum(per_pair.values())
    total_pairs = len(per_pair)
    print(f"rename-noise hunks:            {noise_hunks:>8,}")
    print(f"distinct renamed bindings:     {total_pairs:>8,}  (unique old->new pairs)")
    print(f"total rename occurrences:      {total_occ:>8,}  (~diff lines)")
    print()
    print("root-cause buckets (by occurrences | distinct bindings):")
    for bkt in ("transfer-gap", "asymmetric", "minifier-reroll"):
        occ = per_bucket_occ[bkt]
        pairs = len(per_bucket_pairs[bkt])
        pct = 100 * occ / total_occ if total_occ else 0
        print(f"  {bkt:<18} {occ:>8,} occ ({pct:4.1f}%) | {pairs:>6,} bindings")
    print()
    print(f"top {top_n} noise-contributing renames (occurrences | bucket):")
    for (old, new), n in per_pair.most_common(top_n):
        print(f"  {n:>5}  {old:<24} -> {new:<28} [{bucket(old, new)}]")


if __name__ == "__main__":
    main(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 30)
