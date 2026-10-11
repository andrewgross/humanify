"""REALISTIC BOUNDARIES: the combo bundles segmented the way a real (label-free) bundle must be - the lab's
cc-layout.mjs (exact CJS factories, lazy-ESM groups ending at their __esm marker, eager code unsegmented) -
then every segment hashed and looked up in the index from the single-package apps. Ground truth from the
labels. Measures how much library code whole-module hashing can reach without sourcemap boundaries.
Output: results/realistic_seg.json + printed table."""
import json, glob, collections, subprocess, sys, os
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib, HERE, LAB

rows = [r for r in load() if r['ok'] and is_lib(r) and r['file'] and not r['app'].startswith('combo-')]
IDX = {'bun-1.4.0_min': lambda c: c.startswith('bun') and c.endswith('/min'), 'esbuild-0.28.2_min': lambda c: c == 'esbuild-0.28.2/min'}
MINLEN = 400
out = {}
for cfgkey, pred in IDX.items():
    idx = collections.defaultdict(set)
    for r in rows:
        if pred(r['cfg']):
            idx[r['normM']].add(r['pkg'])
    tot = collections.Counter()
    for lay in sorted(glob.glob(f'{HERE}/seg/{cfgkey}-*.json')):
        app = lay.split(f'{cfgkey}-')[1][:-5]
        d = json.load(open(lay))
        if not d['modules']:
            tot['bundlesNoHelpers'] += 1
            continue
        cfgdir = cfgkey.replace('_', '/')
        bdir = f'{LAB}/builds/{cfgdir}/{app}/main'
        labels = [json.loads(l) for l in open(f'{bdir}/labels.jsonl')]
        data = open(d['input'], 'rb').read()
        os.makedirs(f'{HERE}/seg/mods/{cfgkey}-{app}', exist_ok=True)
        jobs = []
        for i, m in enumerate(d['modules']):
            p = f'{HERE}/seg/mods/{cfgkey}-{app}/{i:05d}.js'
            open(p, 'wb').write(data[m['start']:m['end']])
            jobs.append(json.dumps({'id': i, 'path': p, 'kind': m['kind'], 'bundle': d['input']}))
        res = subprocess.run([f'{HERE}/fphash/target/release/fphash'], input='\n'.join(jobs), capture_output=True, text=True)
        hs = {json.loads(l)['id']: json.loads(l) for l in res.stdout.splitlines()}
        libbytes = sum(l['bytes'] for l in labels if not l['pkg'].startswith('<'))
        tot['libBytes'] += libbytes
        tot['libBytes:' + 'eager'] += sum(l['bytes'] for l in labels if not l['pkg'].startswith('<') and l['kind'] == 'eager-esm')
        tot['libBytes:' + 'segmented'] += 0
        for i, m in enumerate(d['modules']):
            inside = [l for l in labels if l['start'] >= m['start'] and l['end'] <= m['end']]
            pk = {l['pkg'] for l in inside if not l['pkg'].startswith('<')}
            appb = sum(l['bytes'] for l in inside if l['pkg'] == '<app>')
            exact = len(inside) == 1 or (len(pk) == 1 and all(l['file'] == inside[-1]['file'] for l in inside if not l['pkg'].startswith('<')))
            tot['segments'] += 1
            tot['segmentsExact'] += exact
            tot['segBytesLib'] += sum(l['bytes'] for l in inside if not l['pkg'].startswith('<'))
            h = hs.get(i)
            if not h or not h['ok'] or h['normLen'] < MINLEN or h['normM'] not in idx:
                continue
            cands = idx[h['normM']]
            tot['hits'] += 1
            tot['hitLibBytes'] += sum(l['bytes'] for l in inside if not l['pkg'].startswith('<'))
            tot['hitAppBytes'] += appb
            if pk and pk <= cands:
                tot['hitCorrect'] += 1
            elif pk & cands:
                tot['hitPartlyCorrect'] += 1
            else:
                tot['hitWrong'] += 1
    out[cfgkey] = dict(tot)
    t = tot
    print(f"{cfgkey}: segments {t['segments']} ({t['segmentsExact']} hold exactly one library file or app piece); library bytes {t['libBytes']:,} "
          f"(eager, never segmented: {100*t['libBytes:eager']/t['libBytes']:.1f}%); library bytes inside segments {100*t['segBytesLib']/t['libBytes']:.1f}%")
    print(f"   hits {t['hits']} -> library bytes recognised {100*t['hitLibBytes']/t['libBytes']:.1f}%, correct {t['hitCorrect']}, partly {t['hitPartlyCorrect']}, WRONG {t['hitWrong']}, app bytes inside hits {t['hitAppBytes']}; bundles where helpers were not found: {t['bundlesNoHelpers']}")
json.dump(out, open(f'{HERE}/results/realistic_seg.json', 'w'), indent=1)
