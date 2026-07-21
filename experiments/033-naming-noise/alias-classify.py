#!/usr/bin/env python3
"""Split a hop's naming churn into require-alias churn (split/file-relocation,
Lever B / #4 territory) vs true binding-rename churn (Lever A territory).

Reads a unified diff on stdin; needs the NEW tree root as argv[1] so it can
tell which changed identifiers are `const X = require(...)` aliases.

    experiments/033-naming-noise/view-diff.sh <base> <new> | \
        experiments/033-naming-noise/alias-classify.py <new-tree-root>

Emits: distinct churned bindings + line-occurrences, split alias vs other, and
a line-weighted histogram (the churn is power-law — a few heavily-imported
modules dominate the alias bucket).
"""
import difflib
import os
import re
import sys
from collections import Counter

TOKEN = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*")
KW = {
    "var", "let", "const", "function", "return", "if", "else", "for", "while",
    "new", "typeof", "this", "null", "true", "false", "void", "throw", "try",
    "catch", "async", "await", "class", "import", "export", "from", "as",
    "require", "module", "exports", "of", "in", "default", "get", "set",
}
REQUIRE_ALIAS = re.compile(r"\b(?:const|var|let)\s+([A-Za-z_$][\w$]*)\s*=\s*require\(")


def collect_aliases(tree_root: str) -> set[str]:
    aliases: set[str] = set()
    for root, _, files in os.walk(tree_root):
        for f in files:
            if not f.endswith(".js"):
                continue
            try:
                txt = open(os.path.join(root, f), encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            for m in REQUIRE_ALIAS.finditer(txt):
                aliases.add(m.group(1))
    return aliases


def rename_pairs(stream) -> Counter:
    pairs: Counter = Counter()
    minus: list[str] = []
    plus: list[str] = []

    def toks(lines):
        out = []
        for l in lines:
            out.extend(TOKEN.findall(l))
        return out

    def flush():
        a, b = toks(minus), toks(plus)
        for op, i1, i2, j1, j2 in difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_opcodes():
            if op == "replace" and (i2 - i1) == (j2 - j1):
                for oa, ob in zip(a[i1:i2], b[j1:j2]):
                    if oa != ob and oa not in KW and ob not in KW:
                        pairs[(oa, ob)] += 1
        minus.clear()
        plus.clear()

    for line in stream:
        if line.startswith(("diff ", "index ", "--- ", "+++ ", "@@")):
            flush()
        elif line.startswith("-"):
            minus.append(line[1:])
        elif line.startswith("+"):
            plus.append(line[1:])
        else:
            flush()
    flush()
    return pairs


def main():
    if len(sys.argv) < 2:
        print("usage: alias-classify.py <new-tree-root>  < unified.diff", file=sys.stderr)
        sys.exit(2)
    aliases = collect_aliases(sys.argv[1])
    pairs = rename_pairs(sys.stdin)

    alias_lines = alias_ct = other_lines = other_ct = 0
    alias_hist: Counter = Counter()
    for (o, n), c in pairs.items():
        if n in aliases or o in aliases:
            alias_lines += c
            alias_ct += 1
            alias_hist[(o, n)] = c
        else:
            other_lines += c
            other_ct += 1
    tot = alias_lines + other_lines or 1
    print(f"distinct churned bindings: {len(pairs)}  |  total churn line-occurrences: {alias_lines + other_lines}")
    print(f"  require-alias churn (split/relocation — Lever B/#4): {alias_ct} bindings, {alias_lines} lines ({100 * alias_lines / tot:.0f}%)")
    print(f"  true binding rename (Lever A):                        {other_ct} bindings, {other_lines} lines ({100 * other_lines / tot:.0f}%)")
    print("  top alias churners (line-weighted):")
    for (o, n), c in alias_hist.most_common(15):
        print(f"      {c:>4}  {o} -> {n}")


if __name__ == "__main__":
    main()
