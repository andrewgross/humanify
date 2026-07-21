#!/usr/bin/env python3
"""Token-level diff classifier — robust to packed lines (the tool-registry line
that hides dozens of renames on one physical line).

For each hunk: tokenize all '-' content and all '+' content into token streams,
run difflib.SequenceMatcher, and classify each opcode:
  equal        -> unchanged (ignored)
  replace of exactly one identifier by one identifier (both non-keyword) -> RENAME
      (cross-version-matched code: a same-position identifier swap is a rename,
       not a call-target change) — sub-typed by the token shape.
  everything else (insert/delete, or replace touching literals/operators/
      keywords/punctuation or spanning multiple tokens) -> STRUCTURAL (real change)

Reports token counts, and — because renames are what we care about — the number
of DISTINCT (old->new) rename pairs, which is the honest "how many bindings
churned" figure (one binding renamed on 50 lines counts once)."""
import difflib
import re
import sys
from collections import Counter

TOKEN = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*|\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`|\d+\.?\d*|[^\sA-Za-z0-9_$]")
IDENT = re.compile(r"^[A-Za-z_$][A-Za-z0-9_$]*$")
KEYWORDS = {
    "var","let","const","function","return","if","else","for","while","do","switch",
    "case","break","continue","new","typeof","instanceof","in","of","this","null",
    "true","false","undefined","void","delete","throw","try","catch","finally",
    "async","await","yield","class","extends","super","import","export","default",
    "from","as","get","set","static","require","module","exports",
}
ORD = re.compile(r".*\d$")
VALSUF = re.compile(r".*(Val|Var|Item|Module|Handler|Provider|Instance|Value|Fn|Result|Data|Obj|Component)$")

def toks(lines):
    out = []
    for l in lines:
        out.extend(TOKEN.findall(l))
    return out

def is_ident(t):
    return bool(IDENT.match(t)) and t not in KEYWORDS

def sub(old, new):
    if ORD.match(old) or ORD.match(new):
        return "ordinal-mint"
    if VALSUF.match(old) or VALSUF.match(new):
        return "generic-suffix"
    return "descriptive-flip"

def classify(minus, plus, rename_pairs, sub_counts, tallies):
    a, b = toks(minus), toks(plus)
    sm = difflib.SequenceMatcher(a=a, b=b, autojunk=False)
    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op == "equal":
            continue
        if op == "replace" and (i2 - i1) == (j2 - j1):
            # element-wise: identifier↔identifier = rename, else structural
            for oa, ob in zip(a[i1:i2], b[j1:j2]):
                if oa == ob:
                    continue
                if is_ident(oa) and is_ident(ob):
                    tallies["rename_tok"] += 1
                    rename_pairs[(oa, ob)] += 1
                    sub_counts[sub(oa, ob)] += 1
                else:
                    tallies["structural_tok"] += 1
        else:
            tallies["structural_tok"] += (i2 - i1) + (j2 - j1)

def main():
    rename_pairs = Counter(); sub_counts = Counter()
    tallies = Counter()
    minus, plus = [], []
    def flush():
        if minus or plus:
            classify(minus, plus, rename_pairs, sub_counts, tallies)
    for line in sys.stdin:
        if line.startswith(("diff ","index ","--- ","+++ ","@@")):
            flush(); minus.clear(); plus.clear(); continue
        if line.startswith("-"): minus.append(line[1:])
        elif line.startswith("+"): plus.append(line[1:])
        else: flush(); minus.clear(); plus.clear()
    flush()

    rt = tallies["rename_tok"]; st = tallies["structural_tok"]
    tot = rt + st
    print(f"changed tokens: {tot}")
    print(f"  structural (real change): {st:>7}  ({100*st/tot:.1f}%)")
    print(f"  rename (naming noise):    {rt:>7}  ({100*rt/tot:.1f}%)")
    print(f"  DISTINCT rename pairs (bindings that churned): {len(rename_pairs)}")
    for k in sorted(sub_counts, key=lambda x: -sub_counts[x]):
        print(f"      {k:<16} {sub_counts[k]:>6} tok occurrences")
    print("  top 15 churned bindings (old->new x line-occurrences):")
    for (o, n), c in rename_pairs.most_common(15):
        print(f"      {o:>28} -> {n:<28} x{c}")

if __name__ == "__main__":
    main()
