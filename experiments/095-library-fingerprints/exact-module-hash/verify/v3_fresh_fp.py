"""Fresh-sample precision test. Index = the agent's own index (lab, all Bun minified builds, normM, purity>=0.9).
Query = spans of 400 NEW packages (npm top-download 150-3000, none of the lab's 176), bundled by Bun 1.4.0 --minify,
labelled by the lab's own label.mjs. Code from packages the index has never seen plays the role of app code:
any hit on it names the WRONG package. Distinguish real collisions from vendored copies by comparing source text."""
import json, sys, collections, hashlib, glob, os, re
sys.path.insert(0, 'scripts')
from load import load, is_lib
F = 'verify/fresh'
lab = [r for r in load() if r['ok'] and is_lib(r) and r['file'] and (r['purity'] is None or r['purity'] >= 0.9)]
labpkgs = {r['pkg'] for r in lab}
idx = collections.defaultdict(set); idxV = collections.defaultdict(set); idxC = collections.defaultdict(set)
for r in lab:
    if r['cfg'].startswith('bun') and r['cfg'].endswith('/min'):
        idx[r['normM']].add((r['pkg'], r['ver'], r['file'], r['textPath']))
        idxV[r['normV']].add(r['pkg'])
        idxC[r['coreM']].add(r['pkg'])
srcsha = json.load(open('data/src-sha.json'))
hs = {}
for l in open(f'{F}/hash.jsonl'):
    h = json.loads(l); hs[h['id']] = h
labels = [json.loads(l) for l in open(f'{F}/labels-all.jsonl')]

def srcfile(r):
    base = f"{F}/pkgs/{r['app']}/node_modules"
    for c in [f"{base}/{r['pkg']}"] + glob.glob(f"{base}/**/node_modules/{r['pkg']}", recursive=True):
        try:
            if json.load(open(f'{c}/package.json')).get('version') == r['pkgVersion'] and os.path.exists(f"{c}/{r['file']}"):
                return f"{c}/{r['file']}"
        except Exception: pass
FLOORS = [3, 200, 400, 600, 800, 1600]
st = {f: collections.Counter() for f in FLOORS}
fps = []
seen = set()
for i, r in enumerate(labels):
    if r['bundlerVersion'] != '1.4.0' or r['pkg'].startswith('<') or not r['file']: continue
    if r['purity'] is not None and r['purity'] < 0.9: continue
    if r['kind'] not in ('cjs', 'lazy-esm', 'eager-esm'): continue
    h = hs.get(i)
    if not h or not h['ok']: continue
    unseen = r['pkg'] not in labpkgs
    key = (r['pkg'], r['pkgVersion'], r['file'], r['kind'], h['normM'])
    if key in seen: continue
    seen.add(key)
    for f in FLOORS:
        if h['normLen'] < f: continue
        s = st[f]
        s['unseen' if unseen else 'seen'] += 1
        if unseen and h['normM'] in idx:
            s['unseenHit'] += 1
            if h['normV'] in idxV: s['unseenHitStrAgree'] += 1
            if f == 400: fps.append((h['normLen'], r, h))
for f in FLOORS:
    s = st[f]
    print(f"normLen>={f:<5d} unseen-package spans {s['unseen']:6d}  hit the lab index: {s['unseenHit']:4d} ({100*s['unseenHit']/max(1,s['unseen']):.2f}%), "
          f"of which strings also agree (normV): {s['unseenHitStrAgree']}")
# classify the floor-400 hits: vendored copy (same source text as an indexed file) or a true collision
print('\nhits at the recommended floor (normLen>=400), unseen packages:')
rows = []
for nl, r, h in sorted(fps, key=lambda x: x[0]):
    sp = srcfile(r)
    ss = hashlib.sha256(open(sp, 'rb').read()).hexdigest()[:16] if sp else None
    cands = idx[h['normM']]
    same_src = any(srcsha.get(f'{p}@{v}/{fl}') == ss for p, v, fl, _ in cands)
    cand = sorted({f'{p}@{v}/{fl}' for p, v, fl, _ in cands})
    strs = h['normV'] in idxV
    rows.append({'normLen': nl, 'bytes': r['bytes'], 'pkg': f"{r['pkg']}@{r['pkgVersion']}", 'file': r['file'], 'kind': r['kind'],
                 'sameSourceText': same_src, 'stringsAgree': strs, 'candidates': cand[:3], 'nCand': len(cand),
                 'text': open(f"{F}/{r['textPath']}").read()[:300], 'candText': open('/work/exp095-library-fingerprints/lab/' + sorted(cands)[0][3]).read()[:300]})
for x in rows:
    print(f"  {x['normLen']:5d} {x['bytes']:6d}B {x['kind']:9s} {x['pkg']}/{x['file']}  -> {x['candidates'][0]} (+{x['nCand']-1})  sameSrc={x['sameSourceText']} strAgree={x['stringsAgree']}")
json.dump({'byFloor': {f: dict(st[f]) for f in FLOORS}, 'floor400Hits': rows}, open('verify/v3_fresh_fp.json', 'w'), indent=1)
