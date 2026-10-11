"""Hash every factory of the highlight.js builds, then look up Claude Code's 188 grammar positives.
Usage: python3 hljs_recall.py   (after build.sh and cc-layout.mjs per version; see README)"""
import json, glob, os, subprocess, collections
HERE = '/work/exp095-library-fingerprints/exact-module-hash'
LAB = '/work/exp095-library-fingerprints/lab'
jobs = []
for lay in sorted(glob.glob(f'{HERE}/hljs/v*/layout.json')):
    v = lay.split('/')[-2][1:]
    d = json.load(open(lay))
    data = open(d['input'], 'rb').read()
    os.makedirs(f'{HERE}/hljs/v{v}/mods', exist_ok=True)
    for i, m in enumerate(d['modules']):
        p = f'{HERE}/hljs/v{v}/mods/{i:04d}.js'
        open(p, 'wb').write(data[m['start']:m['end']])
        jobs.append({'id': f'{v}:{i}', 'path': p, 'kind': 'cjs', 'bundle': d['input']})
r = subprocess.run([f'{HERE}/fphash/target/release/fphash'], input='\n'.join(json.dumps(j) for j in jobs),
                   capture_output=True, text=True)
hs = [json.loads(l) for l in r.stdout.splitlines()]
idx = collections.defaultdict(lambda: collections.defaultdict(set))
for h in hs:
    if h['ok']:
        for var in ['coreM', 'normM', 'normV', 'factM']:
            idx[var][h[var]].add(h['id'].split(':')[0])
res = {}
for ver in ['2.1.198', '2.1.216']:
    P = [json.loads(l) for l in open(f'{LAB}/realworld/cc-{ver}/positives.jsonl')]
    H = {json.loads(l)['id']: json.loads(l) for l in open(f'{HERE}/data/cc-{ver}-hash.jsonl')}
    hl = [(i, p) for i, p in enumerate(P) if p['pkg'] == 'highlight.js']
    print(f'\nClaude Code {ver}: {len(hl)} highlight.js grammar factories')
    for var in ['coreM', 'normM', 'normV', 'factM']:
        hit = collections.Counter()
        n = 0
        for i, p in hl:
            vs = idx[var].get(H[f'positives:{i}'][var])
            if vs:
                n += 1
                hit[','.join(sorted(vs, key=lambda x: [int(t) for t in x.split('.')]))] += 1
        print(f'  {var}: {n}/{len(hl)} found; matching version sets: {hit.most_common(6)}')
        res[f'{ver}|{var}'] = {'found': n, 'of': len(hl), 'versionSets': hit.most_common(20)}
json.dump(res, open(f'{HERE}/results/hljs_recall.json', 'w'), indent=1)
