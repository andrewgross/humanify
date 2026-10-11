"""VERIFY: is each Claude Code src/ statement the index claims (recommended rule) really library code?
Oracle = literal sets of statements of the CLAIMED package in real minified builds (all lab builds + Bun 1.4.0 builds of
the pkgs-cc versions, verify/ccpk). A claim is CORROBORATED if some oracle statement of that package has literal-set
Jaccard >= 0.8 with it. The rest are listed for reading (verify/cc-uncorroborated.txt)."""
import json, collections, sys
V = sys.argv[1] if len(sys.argv) > 1 else '2.1.216'
SUF = sys.argv[2] if len(sys.argv) > 2 else '-plus-capvocab-origin'
claims = [json.loads(l) for l in open(f'results/cc-claims-{V}{SUF}.jsonl')]
own = [c for c in claims if c['kind'] == 'src-stmt' and c['claim']]
want = {c['claim'] for c in own}
cc = {}
for l in open('data/cc.jsonl'):
    if not l.strip(): continue
    r = json.loads(l)
    if r['cc'] == V and r['set'] == 'stmt': cc[r['startChar']] = frozenset(r['lits'])
# oracle
sha_pkg = collections.defaultdict(set)
for l in open('data/stmts.jsonl'):
    if not l.strip(): continue
    r = json.loads(l)
    if r['pkg'] in want and (r['purity'] is None or r['purity'] >= 0.9): sha_pkg[r['sha']].add(r['pkg'])
orc = collections.defaultdict(set)
for l in open('data/stmt-lits.jsonl'):
    if not l.strip(): continue
    r = json.loads(l)
    if r['sha'] in sha_pkg and r['lits']:
        for p in sha_pkg[r['sha']]: orc[p].add(frozenset(r['lits']))
for l in open('verify/ccpk/out.jsonl'):
    if not l.strip(): continue
    if not l.strip(): continue
    r = json.loads(l)
    if r['pkg'] in want and r['lits'] and (r['purity'] or 0) >= 0.9: orc[r['pkg']].add(frozenset(r['lits']))
inv = {p: collections.defaultdict(list) for p in orc}
for p, sets in orc.items():
    for s in sets:
        for w in s: inv[p][w].append(s)
st = collections.Counter(); unc = []
for c in own:
    q = cc[c['startChar']]
    best = 0.0
    cands = set()
    for w in q: cands.update(inv.get(c['claim'], {}).get(w, ()))
    for s in cands:
        j = len(q & s) / len(q | s)
        if j > best: best = j
    c['jac'] = round(best, 3)
    b = 'jac>=0.8' if best >= 0.8 else ('0.5-0.8' if best >= 0.5 else '<0.5')
    st[b + '-n'] += 1; st[b + '-B'] += c['bytes']
    if best < 0.8: unc.append(c)
print(V, SUF, 'own claims', len(own), dict(st))
code = open(f'/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-{V}/binary-decompiled/src/entrypoints/index.js').read()
with open(f'verify/cc-uncorroborated-{V}.txt', 'w') as o:
    for c in sorted(unc, key=lambda c: c['jac']):
        o.write(f"## {c['claim']} jac={c['jac']} n={c['n']} tot={c['tot']} top={c['top'][:3]} bytes={c['bytes']} @{c['startChar']}\n{code[c['startChar']:c['endChar']][:700]}\n\n")
json.dump({'stats': dict(st), 'uncorroborated': [{k: c[k] for k in ('claim','jac','n','tot','top','bytes','startChar')} for c in unc]}, open(f'verify/cc-oracle-{V}.json', 'w'), indent=1)
