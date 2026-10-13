"""Eager (scope-hoisted) ES modules split into pieces by other modules' statements, and tree-shaking
('some' = 3 named imports vs 'imp' = whole namespace). Printed only (results/eager_parts.txt)."""
import sys, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib
rows = [r for r in load() if r['ok'] and is_lib(r) and r['file']]
for cfg in ['bun-1.4.0/min', 'esbuild-0.28.2/min']:
    for scope, pred in [('all apps', lambda r: True), ('combo apps', lambda r: r['app'].startswith('combo-'))]:
        rs = [r for r in rows if r['cfg'] == cfg and pred(r) and r['kind'] == 'eager-esm']
        split = [r for r in rs if r['parts'] > 1]
        files = {(r['app'], r['mode'], r['pkg'], r['ver'], r['file']) for r in rs}
        sfiles = {(r['app'], r['mode'], r['pkg'], r['ver'], r['file']) for r in split}
        print(f"{cfg} {scope}: eager spans {len(rs)}, in split files {len(split)} ({100*len(split)/max(len(rs),1):.1f}%), "
              f"files {len(files)}, split files {len(sfiles)} ({100*len(sfiles)/max(len(files),1):.1f}%), "
              f"bytes in split files {100*sum(r['bytes'] for r in split)/max(sum(r['bytes'] for r in rs),1):.1f}%")
cfg = 'bun-1.4.0/min'
A = {(r['app'], r['pkg'], r['ver'], r['file']): r for r in rows if r['cfg'] == cfg and r['mode'] == 'imp' and r['parts'] == 1}
B = {(r['app'], r['pkg'], r['ver'], r['file']): r for r in rows if r['cfg'] == cfg and r['mode'] == 'some' and r['parts'] == 1}
ks = [k for k in A if k in B and A[k]['kind'] == 'eager-esm' and B[k]['kind'] == 'eager-esm']
diff = [k for k in ks if B[k]['normM'] != A[k]['normM']]
apps = {k[0] for k in B}
print(f"{cfg} tree-shaking: eager files in both imp and some: {len(ks)}; normM differs: {len(diff)} ({100*len(diff)/len(ks):.1f}%)")
print(f"   files bundled: imp {sum(1 for k in A if k[0] in apps)} vs some {len(B)} in the same {len(apps)} apps")
for cfg in ['bun-1.4.0/min', 'bun-1.4.0/unmin', 'esbuild-0.28.2/min']:
    rs = [r for r in rows if r['cfg'] == cfg and r['kind'] == 'lazy-esm']
    nm = [r for r in rs if r['marker'] is False]
    print(f"{cfg} lazy ESM without an __esm end marker: {len(nm)}/{len(rs)} ({100*len(nm)/len(rs):.1f}%), "
          f"{100*sum(r['bytes'] for r in nm)/sum(r['bytes'] for r in rs):.1f}% of lazy bytes")
