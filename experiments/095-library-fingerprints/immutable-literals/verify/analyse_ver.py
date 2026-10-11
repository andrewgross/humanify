"""VERIFY: recall when the exact version is NOT in the index (in-major unseen version / unseen major).
Statements owned by the target package at the target version (sourcemap), dedup by literal set."""
import json, collections, hashlib
src = [json.loads(l) for l in open('verify/ver-all.jsonl') if l.strip()]
sc = [json.loads(l) for l in open('verify/ver-scored.jsonl')]
assert len(src) == len(sc)
seen = {}
for s, r in zip(src, sc):
    if s['pkg'] != s['target'] or s['ver'] != s['targetVer']: continue
    if r['purity'] is not None and r['purity'] < 0.9: continue
    k = (s['target'], s['targetVer'], hashlib.sha1('\n'.join(sorted(s['lits'])).encode('utf-8','surrogatepass')).hexdigest())
    if k in seen: continue
    seen[k] = (s, r)
agg = collections.defaultdict(collections.Counter)
for (s, r) in seen.values():
    for g in (s['cat'], 'ALL', f"{s['target']}@{s['targetVer']} [{s['cat']}]"):
        a = agg[g]; a['n'] += 1; a['B'] += r['bytes']
        if r['claim'] == s['target']: a['tpn'] += 1; a['tpB'] += r['bytes']
        elif r['claim']: a['wrongn'] += 1; a['wrongB'] += r['bytes']
        if r['n'] > 5: a['n6'] += 1; a['B6'] += r['bytes']; a['tpB6'] += r['bytes'] if r['claim'] == s['target'] else 0
out = {}
for g, a in sorted(agg.items(), key=lambda x: (x[0] not in ('ALL','in','newmajor','newminor0'), x[0])):
    out[g] = dict(stmts=a['n'], KB=round(a['B']/1e3), stmtRecall=round(a['tpn']/a['n'],3), byteRecall=round(a['tpB']/max(a['B'],1),3),
                  byteRecall_gt5lits=round(a['tpB6']/max(a['B6'],1),3), wrongStmts=a['wrongn'], wrongKB=round(a['wrongB']/1e3,1))
    print(f"{g:45s}", out[g])
json.dump(out, open('verify/ver-summary.json','w'), indent=1)
