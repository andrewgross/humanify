"""VERIFY: plain-margin vs origin tie-break on the independent sets (out-of-index precision, unseen-version recall)."""
import json, sys, collections, hashlib
sys.path.insert(0, 'scripts')
import importlib, os
def claims(r, rule):
    os.environ['CLAIM'] = rule
    import recognise; importlib.reload(recognise) if False else None
from recognise import claim_origin, claim
T, C, M = 20, 0.9, 10
def cl(r, rule): return claim_origin(r['top'], r['topLen'], r['tot'], T, C, M) if rule == 'origin' else claim(r['top'], r['tot'], T, C, M)
COPY = set(map(tuple, json.load(open('verify/copy-pairs.json'))))
src = [json.loads(l) for l in open('verify/neg-all.jsonl') if l.strip()]
sc = [json.loads(l) for l in open('verify/neg-scored.jsonl')]
seen = {}
for s, r in zip(src, sc):
    if (r['purity'] is not None and r['purity'] < 0.9) or r['pkg'] == '<unmapped>' or r['inIndex'] or r['pkg'] == '<app>': continue
    k = (r['pkg'], hashlib.sha1('\n'.join(sorted(s['lits'])).encode('utf-8', 'surrogatepass')).hexdigest())
    seen.setdefault(k, r)
for rule in ('plain', 'origin'):
    c = collections.Counter()
    for r in seen.values():
        x = cl(r, rule)
        if x and x != r['pkg']: c['copy' if (r['pkg'], x) in COPY else 'lookalike'] += 1
    print('out-of-index', rule, len(seen), dict(c))
src = [json.loads(l) for l in open('verify/ver-all.jsonl') if l.strip()]
sc = [json.loads(l) for l in open('verify/ver-scored.jsonl')]
seen = {}
for s, r in zip(src, sc):
    if s['pkg'] != s['target'] or s['ver'] != s['targetVer'] or (r['purity'] is not None and r['purity'] < 0.9): continue
    seen.setdefault((s['target'], s['targetVer'], hashlib.sha1('\n'.join(sorted(s['lits'])).encode('utf-8', 'surrogatepass')).hexdigest()), (s, r))
for rule in ('plain', 'origin'):
    B = tp = 0
    for s, r in seen.values():
        B += r['bytes']; tp += r['bytes'] if cl(r, rule) == s['target'] else 0
    print('unseen-version byte recall', rule, round(tp / B, 3))
