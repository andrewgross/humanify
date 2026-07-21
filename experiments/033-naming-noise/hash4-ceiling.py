#!/usr/bin/env python3
"""Lever #4 ceiling — how much require-alias churn is "recognizable module,
relocated/renamed file" (which #4's export-set-keyed alias inheritance fixes)
vs genuinely new/restructured (which nothing cheap fixes).

A require-alias is a pure function of the target file's PATH, so it churns iff
the module's exports moved to a differently-named file. #4 keys the inherited
alias on the module's EXPORT-SET signature instead of the path, so it survives a
file move/rename as long as the export set is recognizable across versions.

Method (two shipped archive trees, same-era split code → real churn, no
confound): for every src file, signature = sorted export names. Then weight each
216 target file by how many importers `require()` it (that's the alias-churn
weight), and bucket by whether its export-set existed in 215 at the SAME path
(stable), a DIFFERENT path (relocated/renamed — #4-addressable), or nowhere
(new/restructured).

    python3 hash4-ceiling.py <tree-215-root> <tree-216-root>
"""
import os
import re
import sys
from collections import Counter, defaultdict

EXPORT = re.compile(r'Object\.defineProperty\(module\.exports,\s*"([^"]+)"')
REQUIRE = re.compile(r'require\("([^"]+)"\)')


def rel_files(root: str) -> list[str]:
    out = []
    for dirpath, _, files in os.walk(root):
        for f in files:
            if f.endswith(".js"):
                out.append(os.path.relpath(os.path.join(dirpath, f), root))
    return out


def signature(path: str) -> tuple:
    try:
        txt = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return ()
    return tuple(sorted(set(EXPORT.findall(txt))))


def build(root: str) -> dict[str, frozenset]:
    """{relative path → export-name set} for exporting src files."""
    out = {}
    for rel in rel_files(root):
        exports = set(EXPORT.findall(open(os.path.join(root, rel), encoding="utf-8", errors="replace").read())) if os.path.exists(os.path.join(root, rel)) else set()
        if exports:  # non-exporting/vendor files aren't the alias-churn population
            out[rel] = frozenset(exports)
    return out


# Minimum export-set overlap to call two files "the same module across versions".
FUZZY_FLOOR = 0.5


def best_match(sig: frozenset, index: dict, by_len: dict) -> tuple[str | None, float]:
    """Best-Jaccard 215 file for a 216 export set. Prefiltered by candidates
    that share at least one export (an inverted index) so it isn't O(n²)."""
    from collections import Counter as C
    hits = C()
    for name in sig:
        for cand in by_len.get(name, ()):  # files exporting `name`
            hits[cand] += 1
    best, best_j = None, 0.0
    for cand, inter in hits.items():
        union = len(sig) + len(index[cand]) - inter
        j = inter / union if union else 0.0
        if j > best_j:
            best, best_j = cand, j
    return best, best_j


def count_require_refs(root: str) -> Counter:
    """How many importers require each target file (by resolved relative path)."""
    refs = Counter()
    for rel in rel_files(root):
        full = os.path.join(root, rel)
        try:
            txt = open(full, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        base = os.path.dirname(rel)
        for target in REQUIRE.finditer(txt):
            resolved = os.path.normpath(os.path.join(base, target.group(1)))
            refs[resolved] += 1
    return refs


def main():
    root215, root216 = sys.argv[1], sys.argv[2]
    print(f"#4 ceiling: {root215} vs {root216}  (fuzzy floor {FUZZY_FLOOR})\n")

    sig215 = build(root215)
    sig216 = build(root216)
    refs216 = count_require_refs(root216)

    # Inverted index over 215: export name → files that export it.
    by_name = defaultdict(list)
    for rel, sig in sig215.items():
        for name in sig:
            by_name[name].append(rel)

    # Bucket each 216 exporting file by cross-version recognizability, weighted
    # by importer references (the alias-churn weight).
    stable = relocated = new = 0
    stable_w = relocated_w = new_w = 0
    reloc_examples = []
    for rel, sig in sig216.items():
        w = refs216.get(rel, 0)
        match, j = best_match(sig, sig215, by_name)
        if j < FUZZY_FLOOR:
            new += 1
            new_w += w
        elif match == rel:
            stable += 1
            stable_w += w
        else:
            relocated += 1
            relocated_w += w
            if w:
                reloc_examples.append((w, f"{match} (j={j:.2f})", rel))

    tot_files = stable + relocated + new
    tot_w = stable_w + relocated_w + new_w or 1
    print("exporting files (216), by cross-version export-set identity:")
    print(f"  stable (same path):                 {stable:>5} files")
    print(f"  relocated/renamed (recognizable):   {relocated:>5} files  ← #4-addressable")
    print(f"  new/restructured (unrecognizable):  {new:>5} files")
    print()
    print("weighted by importer require-references (= alias-churn weight):")
    print(f"  stable:                    {stable_w:>6} refs ({100*stable_w/tot_w:.0f}%)  — no churn")
    print(f"  relocated (#4 fixes):      {relocated_w:>6} refs ({100*relocated_w/tot_w:.0f}%)  ← the #4 ceiling")
    print(f"  new/restructured:          {new_w:>6} refs ({100*new_w/tot_w:.0f}%)  — neither B nor #4")
    print()
    print("top relocated-but-recognizable targets (importer-refs, 215-path → 216-path):")
    for w, p215, p216 in sorted(reloc_examples, reverse=True)[:12]:
        print(f"  {w:>4}  {p215}  →  {p216}")


if __name__ == "__main__":
    main()
