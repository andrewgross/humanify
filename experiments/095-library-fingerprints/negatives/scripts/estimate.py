"""Stratified ratio estimate of the library share of CC 2.1.216 src/ bytes from labels.jsonl (stratum = random S0..S3),
with a stratified bootstrap CI (B=20000, seed 95). Also per-package estimated bytes."""
import json, random, collections, sys
meta = json.load(open('data/sample-meta.json'))
rows = [json.loads(l) for l in open('labels.jsonl')]
rows = [r for r in rows if r['stratum'] in meta['strata']]
by = collections.defaultdict(list)
for r in rows: by[r['stratum']].append(r)
TOT = meta['populationBytes']
def cls(r):
    l = r['label']
    if l.startswith('library:'): return 'library'
    return l
def est(sample_by, f):
    tot = 0.0
    for h, m in meta['strata'].items():
        S = sample_by[h]; sb = sum(r['bytes'] for r in S)
        if sb: tot += m['bytes'] * sum(r['bytes'] for r in S if f(r)) / sb
    return tot
def ci(f, B=20000, seed=95):
    rng = random.Random(seed); vals = []
    for _ in range(B):
        sb = {h: [rng.choice(by[h]) for _ in by[h]] for h in by}
        vals.append(est(sb, f) / TOT)
    vals.sort(); return vals[int(0.025 * B)], vals[int(0.975 * B)]
out = {'populationBytes': TOT, 'n': len(rows), 'counts': collections.Counter(cls(r) for r in rows),
       'countsByStratum': {h: dict(collections.Counter(cls(r) for r in by[h])) for h in by}}
for name, f in [('library', lambda r: cls(r) == 'library'),
                ('library+compiler-helper+runtime', lambda r: cls(r) in ('library', 'compiler-helper', 'bundler-runtime')),
                ('app', lambda r: cls(r) == 'app'), ('app-fork', lambda r: cls(r) == 'app' and r.get('fork_of')),
                ('unsure', lambda r: cls(r) == 'unsure')]:
    p = est(by, f) / TOT
    lo, hi = ci(f)
    out[name] = {'share': round(p, 4), 'ci95': [round(lo, 4), round(hi, 4)], 'MB': round(p * TOT / 1e6, 2), 'MBci95': [round(lo * TOT / 1e6, 2), round(hi * TOT / 1e6, 2)]}
pk = collections.Counter(); pn = collections.Counter()
for r in rows:
    if r['pkg']: pn[r['pkg']] += 1
pkgs = {}
for p in pn:
    f = (lambda q: lambda r: r['pkg'] == q)(p)
    v = est(by, f); lo, hi = ci(f, B=4000)
    pkgs[p] = {'items': pn[p], 'estKB': round(v / 1e3), 'ci95KB': [round(lo * TOT / 1e3), round(hi * TOT / 1e3)]}
out['packages'] = dict(sorted(pkgs.items(), key=lambda kv: -kv[1]['estKB']))
fam = lambda p: ('@aws-sdk/* + @smithy/*' if p.startswith(('@aws-sdk/', '@smithy/', '@aws-crypto/')) else '@azure/*' if p.startswith(('@azure/', '@typespec/')) else '@anthropic-ai/* (sdk family)' if p.startswith('@anthropic-ai/') else p)
fams = {}
for F in sorted({fam(p) for p in pn}):
    f = (lambda q: lambda r: r['pkg'] and fam(r['pkg']) == q)(F)
    v = est(by, f); lo, hi = ci(f, B=4000)
    fams[F] = {'estKB': round(v / 1e3), 'ci95KB': [round(lo * TOT / 1e3), round(hi * TOT / 1e3)]}
out['families'] = dict(sorted(fams.items(), key=lambda kv: -kv[1]['estKB']))
json.dump(out, open('results-estimate.json', 'w'), indent=1)
print(json.dumps({k: v for k, v in out.items() if k not in ('packages', 'families')}, indent=1))
for k, v in list(out['families'].items())[:20]: print(k, v)
