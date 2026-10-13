"""Claude Code real-world jobs: every CJS factory and lazy-ESM group of the raw bundle (from the lab's
layout-<v>.json), plus the lab's negatives and negative functions. Writes the span texts under
data/cc-<v>/ and the jobs to data/cc-<v>-jobs.jsonl."""
import json, os, sys
LAB = '/work/exp095-library-fingerprints/lab'
HERE = '/work/exp095-library-fingerprints/exact-module-hash'
for v in sys.argv[1:] or ['2.1.198', '2.1.216']:
    lay = json.load(open(f'{LAB}/realworld/layout-{v}.json'))
    bundle = lay['input']
    data = open(bundle, 'rb').read()
    d = f'{HERE}/data/cc-{v}/mods'
    os.makedirs(d, exist_ok=True)
    with open(f'{HERE}/data/cc-{v}-jobs.jsonl', 'w') as out:
        for i, m in enumerate(lay['modules']):
            p = f'{d}/{i:05d}.js'
            open(p, 'wb').write(data[m['start']:m['end']])
            out.write(json.dumps({'id': f'mod:{i}', 'path': p, 'kind': m['kind'], 'bundle': bundle}) + '\n')
        for name in ['negatives', 'negative-functions', 'positives']:
            for i, l in enumerate(open(f'{LAB}/realworld/cc-{v}/{name}.jsonl')):
                r = json.loads(l)
                kind = r.get('kind', 'other') if name != 'negative-functions' else 'other'
                out.write(json.dumps({'id': f'{name}:{i}', 'path': f"{LAB}/{r['textPath']}", 'kind': kind, 'bundle': bundle}) + '\n')
    print(v, len(lay['modules']), 'modules')
