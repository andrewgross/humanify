"""Join lab labels with fphash output into one compact row list (cached as a pickle)."""
import json, os, pickle

LAB = '/work/exp095-library-fingerprints/lab'
HERE = '/work/exp095-library-fingerprints/exact-module-hash'
CACHE = f'{HERE}/data/rows.pkl'

VARIANTS = ['raw', 'fullStmt', 'fullM', 'fullV', 'coreStmt', 'coreM', 'coreV', 'coreSetStmt', 'coreSetM', 'normStmt', 'normM', 'normV', 'normSetStmt', 'normSetM']


def config_of(r):
    return f"{r['bundler']}-{r['bundlerVersion']}/{'min' if r['minified'] else 'unmin'}"


def load():
    if os.path.exists(CACHE) and os.path.getmtime(CACHE) > os.path.getmtime(f'{HERE}/data/lab-hash.jsonl'):
        return pickle.load(open(CACHE, 'rb'))
    hashes = {}
    for l in open(f'{HERE}/data/lab-hash.jsonl'):
        h = json.loads(l)
        hashes[h['id']] = h
    rows = []
    for i, l in enumerate(open(f'{LAB}/labels/all.jsonl')):
        r = json.loads(l)
        h = hashes.get(i)
        row = {
            'i': i, 'cfg': config_of(r), 'app': r['app'], 'mode': r['mode'], 'pkg': r['pkg'],
            'ver': r['pkgVersion'], 'file': r['file'], 'kind': r['kind'], 'marker': r.get('marker'),
            'part': r['part'], 'parts': r['parts'], 'bytes': r['bytes'], 'purity': r['purity'],
            'textPath': r['textPath'], 'raw': r['sha256'], 'ok': bool(h and h['ok']),
            'srcFormat': r.get('srcFormat'), 'declUnits': r.get('declaratorUnits', 0),
        }
        if h and h['ok']:
            for k in VARIANTS[1:]:
                row[k] = h[k]
            row['nCore'] = h['nCore']; row['nNorm'] = h['nNorm']; row['normLen'] = h['normLen']; row['coreLen'] = h['coreLen']
            row['factory'] = h['factory']
        rows.append(row)
    pickle.dump(rows, open(CACHE, 'wb'))
    return rows


def is_lib(r):
    return not r['pkg'].startswith('<')
