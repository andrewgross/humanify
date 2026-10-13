"""VERIFY: out-of-index precision. Statements owned (by sourcemap) by a package NOT in the index: any claim is false
unless the code is a genuine copy. Dedup by (owner, bytes, head)."""
import json, collections, sys
f = sys.argv[1] if len(sys.argv) > 1 else 'verify/neg-scored.jsonl'
seen = {}
for l in open(f):
    r = json.loads(l)
    if r['purity'] is not None and r['purity'] < 0.9: continue
    k = (r['pkg'], r['bytes'], r['head'])
    if k in seen: continue
    seen[k] = r
R = list(seen.values())
out = collections.Counter(); byc = collections.Counter(); ex = collections.defaultdict(list)
for r in R:
    own = r['pkg']
    if own.startswith('<'): cls = 'app/glue'
    elif r['inIndex']: cls = 'in-index'
    else: cls = 'out-of-index'
    out[cls+'-n'] += 1; out[cls+'-B'] += r['bytes']
    if r['claim']:
        ok = r['claim'] == own
        tag = cls + ('-right' if ok else '-wrong')
        out[tag+'-n'] += 1; out[tag+'-B'] += r['bytes']
        if not ok: byc[(own, r['claim'])] += 1; ex[(own, r['claim'])].append((r['bytes'], r['n'], round(r['tot'],1), r['top'][:2], r['head'][:150]))
for k in sorted(out): print(k, out[k])
print('\nwrong claims by (owner -> claimed):')
for (o, c), n in byc.most_common(60):
    print(f'{n:4d}  {o} -> {c}   e.g.', ex[(o,c)][0])
json.dump({'counts': out, 'pairs': [[o, c, n, ex[(o, c)][:5]] for (o, c), n in byc.most_common()]}, open(f.replace('.jsonl', '-summary.json'), 'w'), indent=1, default=str)
