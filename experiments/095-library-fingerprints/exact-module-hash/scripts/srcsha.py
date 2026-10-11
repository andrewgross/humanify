"""Source-file identity: (pkg, version, file) -> sha256 of the installed source file text.
Used to tell a TRUE collision (different code, same hash) from a duplicate (the same source file
published under two names). Output: data/src-sha.json"""
import json, os, hashlib, glob, sys
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import LAB, HERE

ids = {}
for l in open(f'{LAB}/labels/all.jsonl'):
    r = json.loads(l)
    if r['pkg'].startswith('<') or not r['file']:
        continue
    ids.setdefault((r['pkg'], r['pkgVersion'], r['file']), r['app'])


def find(app, pkg, ver, f):
    p = f'{LAB}/pkgs/{app}/node_modules/{pkg}'
    cands = [p] + glob.glob(f'{LAB}/pkgs/{app}/node_modules/**/node_modules/{pkg}', recursive=True)
    for c in cands:
        try:
            if json.load(open(f'{c}/package.json')).get('version') == ver and os.path.exists(f'{c}/{f}'):
                return f'{c}/{f}'
        except Exception:
            pass
    return None


out, missing = {}, 0
for (pkg, ver, f), app in ids.items():
    p = find(app, pkg, ver, f)
    if p is None:
        missing += 1
        continue
    out[f'{pkg}@{ver}/{f}'] = hashlib.sha256(open(p, 'rb').read()).hexdigest()[:16]
json.dump(out, open(f'{HERE}/data/src-sha.json', 'w'))
print('ids', len(ids), 'resolved', len(out), 'missing', missing)
