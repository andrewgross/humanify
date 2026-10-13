#!/usr/bin/env python3
# usage: python3 scripts/validate.py  -> labels/validation.json
# Independent checks of the sourcemap labels:
#  A. Unminified Bun/esbuild builds print `// <path>` comments before each module; the span's own leading
#     comment must name the same file as the label.
#  B. Each minified build must label the same set of (pkg, version, file) as its unminified twin.
import json, glob, re, collections, os
lab = os.path.dirname(os.path.dirname(os.path.abspath(__file__))); os.chdir(lab)
A = collections.Counter(); Aex = []
B = collections.Counter(); Bex = []
pc = re.compile(r'^// (\S+)$', re.M)
for f in glob.glob('builds/*/unmin/*/*/labels.jsonl'):
    bundler = f.split('/')[1]
    rows = [json.loads(l) for l in open(f)]
    if not bundler.startswith('rollup'):
        for r in rows:
            if r['pkg'].startswith('<'): continue
            t = open(r['textPath']).read()
            # comments inside the span's leading trivia / first lines
            m = [x for x in pc.findall(t[:4000]) if 'node_modules/' in x]
            if not m: A[bundler, 'no-comment'] += 1; continue
            want = r['pkg'] + '/' + r['file']
            ok = any(x.endswith('node_modules/' + want) for x in m)
            A[bundler, 'agree' if ok else 'DISAGREE'] += 1
            if not ok and len(Aex) < 10: Aex.append((r['build'], want, m[:3]))
    mf = f.replace('/unmin/', '/min/')
    if os.path.exists(mf):
        key = lambda r: (r['pkg'], r['pkgVersion'], r['file'])
        u = {key(r) for r in rows if not r['pkg'].startswith('<')}
        m = {key(json.loads(l)) for l in open(mf) if not json.loads(l)['pkg'].startswith('<')}
        B[bundler, 'same-files' if u == m else 'differ'] += 1
        if u != m and len(Bex) < 10: Bex.append((f, sorted(u - m)[:3], sorted(m - u)[:3]))
out = {'A_pathComment': {f'{k[0]}:{k[1]}': v for k, v in A.items()}, 'A_examples': Aex,
       'B_minVsUnminFileSets': {f'{k[0]}:{k[1]}': v for k, v in B.items()}, 'B_examples': Bex}
json.dump(out, open('labels/validation.json', 'w'), indent=1)
print(json.dumps(out, indent=1)[:3000])
