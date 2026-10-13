"""Task 2: Step B (bundler-variation-and-boundaries/stepb.py, verified S3 config) on CC 2.1.216 src with
index A = lab min builds (+ the bvb exactver mini-lab), index B = A + this lane's exact-version mini-lab (exactlab/).
Scored against this lane's labels (random sample, traps, review queue). PYTHONHASHSEED=0.
usage: PYTHONHASHSEED=0 python3 scripts/stepb_cc.py"""
import json, sys, glob, os, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/negatives/scripts')
from stepb import load_units, index_from_builds, detect, UNITS
CONFIG = dict(strategy='S3', nmin=30, mincontent=3, minfncontent=0, support=10)
rel = lambda p: os.path.relpath(p, UNITS)[:-6]
labb = sorted(rel(p) for p in glob.glob(UNITS + '/*/min/*/*.jsonl') if '/cc/' not in p and '/exact/' not in p and '/exactver/' not in p)
bvbex = sorted(rel(p) for p in glob.glob(UNITS + '/exactver/*/min/*/*.jsonl'))
mine = sorted(rel(p) for p in glob.glob(UNITS + '/exact/bun-1.4.0/min/*/*.jsonl'))
mineb = sorted(rel(p) for p in glob.glob(UNITS + '/exact/bun-1.4.0-tbun/min/*/*.jsonl'))
print('builds lab', len(labb), 'bvb-exactver', len(bvbex), 'this lane exact (target node)', len(mine), '(target bun)', len(mineb), flush=True)
units = load_units('cc/min/cc-2.1.216/raw')
raw = [json.loads(l) for l in open(UNITS + '/cc/min/cc-2.1.216/raw.jsonl')]
leaf_i = [r['i'] for r in raw if not (r['d'] == 0 and r['w'])]
assert len(leaf_i) == len(units)
pop = {json.loads(l)['i'] for l in open('data/population.jsonl')}
lab = {}
for l in open('labels.jsonl'):
    r = json.loads(l)
    if r['stratum'] != 'review-queue': lab[r['unit']] = r
traps = {json.loads(l)['unit']: json.loads(l) for l in open('traps.jsonl')}
queue = [json.loads(l) for l in open('data/queue-labels.jsonl')]
fam = lambda p: ('aws/smithy' if p and p.startswith(('@aws-sdk/', '@smithy/', '@aws-crypto/')) else 'azure' if p and p.startswith(('@azure/', '@typespec/')) else p)
res = {}
for name, builds in (('A lab+bvb-exactver', labb + bvbex), ('B A+exact-version mini-lab (target node)', labb + bvbex + mine), ('C B+exact-version mini-lab (target bun)', labb + bvbex + mine + mineb)):
    idx = index_from_builds(builds, 'h3')
    pred = detect(units, idx, **CONFIG)
    P = {i: p for i, p in zip(leaf_i, pred)}
    U = {i: u for i, u in zip(leaf_i, units)}
    claimed = collections.Counter(); claimedN = collections.Counter()
    for i in pop:
        p = P.get(i)
        if p: claimed[p] += U[i]['e'] - U[i]['s']; claimedN[p] += 1
    # scoring against the random-sample labels (S0..S3), byte-weighted within the sample and stratum-weighted
    meta = json.load(open('data/sample-meta.json'))
    by = collections.defaultdict(lambda: collections.Counter())
    false_claims = []; wrongpkg = []
    for i, r in lab.items():
        p = P.get(i); h = r['stratum']; b = r['bytes']; L = r['label'].split(':')[0]
        if L == 'library':
            by[h]['lib'] += b
            if p: by[h]['libFound'] += b
            if p and p == r['pkg']: by[h]['libRight'] += b
            if p and fam(p) == fam(r['pkg']): by[h]['libRightFam'] += b
            if p and fam(p) != fam(r['pkg']): wrongpkg.append({'unit': i, 'start': r['start'], 'end': r['end'], 'bytes': b, 'truth': r['pkg'], 'claimed': p})
        elif L in ('app',):
            by[h]['app'] += b
            if p: by[h]['appFlagged'] += b; false_claims.append({'unit': i, 'start': r['start'], 'end': r['end'], 'bytes': b, 'claimed': p, 'label': r['label'], 'fork_of': r.get('fork_of'), 'reason': r['reason']})
        else:
            by[h]['other'] += b
            if p: by[h]['otherFlagged'] += b; false_claims.append({'unit': i, 'start': r['start'], 'end': r['end'], 'bytes': b, 'claimed': p, 'label': r['label'], 'reason': r['reason']})
    est = lambda k: sum(m['bytes'] * by[h][k] / max(1, sum(by[h][x] for x in ('lib', 'app', 'other'))) for h, m in meta['strata'].items())
    recall = est('libFound') / max(1, est('lib'))
    trapflag = [{'trap': t['trap'], 'unit': i, 'bytes': t['bytes'], 'label': t['label'], 'claimed': P.get(i)} for i, t in traps.items() if P.get(i)]
    qa = collections.Counter()
    qfalse = []
    for q in queue:
        ps = [P.get(i) for i in q['units']]
        hit = any(ps)
        isapp = q['label'] == 'app'
        qa[('app' if isapp else 'lib') + ('-claimed' if hit else '-unclaimed')] += 1
        if isapp and hit: qfalse.append({'queueIndex': q['queueIndex'], 'start': q['start'], 'bytes': q['bytes'], 'claimed': [p for p in ps if p], 'reason': q['reason']})
    res[name] = {'claimedSrcKB_total': round(sum(claimed.values()) / 1e3, 1), 'claimedSrcKB_byPkg': {k: round(v / 1e3, 1) for k, v in claimed.most_common()},
                 'claimedUnits_byPkg': dict(claimedN.most_common()),
                 'sampleRecall_byteWeighted_stratified': round(recall, 4),
                 'sampleRecall_rightPkg': round(est('libRight') / max(1, est('lib')), 4), 'sampleRecall_rightFamily': round(est('libRightFam') / max(1, est('lib')), 4),
                 'sampleLibBytes': sum(by[h]['lib'] for h in by), 'sampleAppBytes': sum(by[h]['app'] for h in by),
                 'sampleAppFlaggedBytes': sum(by[h]['appFlagged'] for h in by),
                 'falseClaims_sample': false_claims, 'wrongPackage_sample': wrongpkg, 'trapsFlagged': trapflag, 'queue': dict(qa), 'queueFalseClaims': qfalse}
    # per-package recall within the sample
    pr = collections.defaultdict(lambda: [0, 0])
    for i, r in lab.items():
        if r['label'].startswith('library:'):
            pr[r['pkg']][0] += r['bytes']
            if P.get(i): pr[r['pkg']][1] += r['bytes']
    res[name]['sampleRecall_byPkg'] = {k: [v[1], v[0], round(v[1] / v[0], 3)] for k, v in sorted(pr.items(), key=lambda kv: -kv[1][0])}
    json.dump(P, open(f'data/stepb-pred-{name[0]}.json', 'w'))
    print(name, json.dumps({k: v for k, v in res[name].items() if k not in ('claimedSrcKB_byPkg', 'claimedUnits_byPkg', 'falseClaims_sample', 'wrongPackage_sample', 'sampleRecall_byPkg', 'queueFalseClaims')}), flush=True)
json.dump(res, open('results-stepb.json', 'w'), indent=1)
