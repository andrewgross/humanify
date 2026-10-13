#!/usr/bin/env python3
# usage: python3 scripts/aggregate.py  -> labels/all.jsonl (every row of every build) + labels/summary.json
import json, glob, collections, os
lab = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(lab)
rows = 0; S = collections.defaultdict(collections.Counter); B = collections.Counter(); pkgv = set(); byt = collections.Counter()
builds = sorted(glob.glob('builds/*/*/*/*/labels.jsonl'))
with open('labels/all.jsonl', 'w') as out:
    for f in builds:
        for line in open(f):
            r = json.loads(line); out.write(line); rows += 1
            key = f"{r['bundler']}-{r['bundlerVersion']}/{'min' if r['minified'] else 'unmin'}"
            B[key] += 0
            lib = not r['pkg'].startswith('<')
            S['kind:' + key][r['kind'] + ('' if r['marker'] is None else (':marked' if r['marker'] else ':unmarked'))] += 1
            S['pkgclass:' + key]['library' if lib else r['pkg']] += 1
            if lib: pkgv.add((r['pkg'], r['pkgVersion'])); byt['library-bytes:' + key] += r['bytes']
            pur = r['purity']
            S['purity:' + key]['none' if pur is None else '1.0' if pur == 1 else '>=0.9' if pur >= 0.9 else '>=0.5' if pur >= 0.5 else '<0.5'] += 1
        B[f.split('/')[1] + '/' + f.split('/')[2]] += 1
summary = {'builds': len(builds), 'rows': rows, 'buildsByBundler': B, 'distinctPkgVersions': len(pkgv),
           'distinctPkgs': len({p for p, _ in pkgv}), 'stats': S, 'bytes': byt}
json.dump(summary, open('labels/summary.json', 'w'), indent=1, default=dict)
print(json.dumps(summary, indent=1, default=dict))
# per-build manifest
with open('labels/builds.jsonl', 'w') as out:
    for f in builds:
        d = os.path.dirname(f); _, bundler, mn, app, mode = d.split('/')
        rows_ = [json.loads(l) for l in open(f)]
        out.write(json.dumps({'build': d[len('builds/'):], 'dir': d, 'bundler': bundler, 'minified': mn == 'min', 'app': app, 'mode': mode,
            'outBytes': os.path.getsize(d + '/out.js'), 'modules': len(rows_),
            'libraryModules': sum(1 for r in rows_ if not r['pkg'].startswith('<')),
            'pkgs': sorted({f"{r['pkg']}@{r['pkgVersion']}" for r in rows_ if not r['pkg'].startswith('<')})}) + '\n')
