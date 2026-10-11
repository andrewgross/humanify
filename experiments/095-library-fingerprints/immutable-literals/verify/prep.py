"""VERIFY: assemble the query files. -> verify/neg-all.jsonl, verify/ver-all.jsonl (target/targetVer/cat columns),
verify/cc216-src.jsonl (every Claude Code 2.1.216 statement outside CJS factories, literals from data/cc.jsonl)."""
import json, glob, os
os.chdir('/work/exp095-library-fingerprints/immutable-literals')
with open('verify/neg-all.jsonl', 'w') as o:
    for f in sorted(glob.glob('verify/neg/part.*.jsonl')): o.write(''.join(l for l in open(f) if l.strip()))
cats = {l.split()[0].replace('/', '_'): l.split()[1] for l in open('verify/ver/specs.txt') if l.strip()}
with open('verify/ver-all.jsonl', 'w') as o:
    for d in sorted(glob.glob('verify/ver/p/*')):
        spec = os.path.basename(d); name, ver = spec.rsplit('@', 1)
        if name.startswith('@'): name = name.replace('_', '/', 1)
        for l in open(d + '/out.jsonl'):
            if not l.strip(): continue
            r = json.loads(l); r.update(target=name, targetVer=ver, cat=cats[spec]); o.write(json.dumps(r) + '\n')
src = {json.loads(l)['startChar'] for l in open('results/cc-claims-2.1.216-plus-capvocab-origin.jsonl') if '"src-stmt"' in l}
with open('verify/cc216-src.jsonl', 'w') as o:
    for l in open('data/cc.jsonl'):
        r = json.loads(l)
        if r['cc'] == '2.1.216' and r['set'] == 'stmt' and r['startChar'] in src:
            o.write(json.dumps({'pkg': '<cc>', 'startChar': r['startChar'], 'bytes': r['bytes'], 'purity': None, 'head': '', 'lits': r['lits']}) + '\n')
