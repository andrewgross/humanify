"""Cluster residual diff hunks: python3 scripts/residual_diff.py <pairs.jsonl> [topN]

Each hunk is abstracted ($n identifiers -> $, string/number literals kept short) and counted, with one example.
"""
import json, sys, difflib, collections, re

path = sys.argv[1]
top = int(sys.argv[2]) if len(sys.argv) > 2 else 40


def ab(t):
    if t.startswith('$'):
        return '$'
    if t[0] in 'SNT' and len(t) > 12:
        return t[0] + '…'
    return t


cnt = collections.Counter()
ex = {}
fns = 0
for line in open(path):
    r = json.loads(line)
    fns += 1
    a, b = r['a'], r['b']
    # compare with identifiers masked so a renumbering does not create a hunk by itself
    ma, mb = [ab(x) for x in a], [ab(x) for x in b]
    sm = difflib.SequenceMatcher(None, ma, mb, autojunk=False)
    seen = set()
    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op == 'equal':
            continue
        sig = (' '.join(ma[i1:i2][:12]), ' '.join(mb[j1:j2][:12]))
        if sig in seen:
            continue
        seen.add(sig)
        cnt[sig] += 1
        if sig not in ex:
            ctx_a = ' '.join(a[max(0, i1 - 6):i2 + 6])
            ctx_b = ' '.join(b[max(0, j1 - 6):j2 + 6])
            ex[sig] = (r['key'], ctx_a[:400], ctx_b[:400])
print(f'{fns} unmatched function pairs; masked-identical pairs = renumbering only')
masked_same = 0
for line in open(path):
    r = json.loads(line)
    if [ab(x) for x in r['a']] == [ab(x) for x in r['b']]:
        masked_same += 1
print('masked-identical (alpha renumbering only):', masked_same)
for sig, c in cnt.most_common(top):
    print(f'\n{c:5d}  A: {sig[0]!r}\n       B: {sig[1]!r}')
    print('       ex', ex[sig][0])
    print('       A>', ex[sig][1])
    print('       B>', ex[sig][2])
