"""Stratified uniform random sample of CC 2.1.216 src/ leaf statements. Seed 95 (exp095), recorded.
strata by statement bytes: S0 <100, S1 100-999, S2 1000-9999, S3 >=10000. Within a stratum: uniform without replacement."""
import json, random
SEED = 95
STRATA = [('S0', 0, 100, 150), ('S1', 100, 1000, 180), ('S2', 1000, 10000, 180), ('S3', 10000, 10**12, 110)]
pop = [json.loads(l) for l in open('data/population.jsonl')]
rng = random.Random(SEED)
out = []
meta = {'seed': SEED, 'populationUnits': len(pop), 'populationBytes': sum(u['bytes'] for u in pop), 'strata': {}}
for name, lo, hi, k in STRATA:
    s = [u for u in pop if lo <= u['bytes'] < hi]
    pick = rng.sample(s, k)
    pick.sort(key=lambda u: u['s'])
    meta['strata'][name] = {'lo': lo, 'hi': hi, 'N': len(s), 'bytes': sum(u['bytes'] for u in s), 'n': k}
    for u in pick:
        u['stratum'] = name
        out.append(u)
json.dump(meta, open('data/sample-meta.json', 'w'), indent=1)
with open('data/sample.jsonl', 'w') as f:
    for u in out: f.write(json.dumps(u) + '\n')
print(json.dumps(meta, indent=1))
