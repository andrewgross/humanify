"""Version neighbourhood for a package from CC src literals.
usage: python3 scripts/versionscan.py <pkg> [minVersionPrefix] ...  (env MAXV=40)
Downloads tarballs (npm pack, no install scripts, nothing executed) of non-prerelease versions published before CC 2.1.216
(2026-07-20), extracts js files, and scores each version by how many CC src/ string literals (>=8 chars, present in >=1
version) it contains. Writes results to data/versionscan/<pkg>.json"""
import json, subprocess, sys, os, re, tarfile, io, collections
CUT = '2026-07-20T20:19'
pkg = sys.argv[1]; prefixes = sys.argv[2:]
MAXV = int(os.environ.get('MAXV', '40'))
t = json.loads(subprocess.run(['npm', 'view', pkg, 'time', '--json'], capture_output=True, text=True).stdout)
vs = [v for v, d in t.items() if v not in ('created', 'modified') and d < CUT and re.fullmatch(r'\d+\.\d+\.\d+', v)]
if prefixes: vs = [v for v in vs if any(v.startswith(p) for p in prefixes)]
key = lambda v: tuple(int(x) for x in v.split('.'))
vs = sorted(vs, key=key)[-MAXV:]
D = f'tarballs/{pkg.replace("/", "__")}'
os.makedirs(D, exist_ok=True)
STR = re.compile(r'"((?:[^"\\\n]|\\.){8,200})"|\'((?:[^\'\\\n]|\\.){8,200})\'|`([^`$\\\n]{8,200})`')
def lits_of_version(v):
    tgz = f'{D}/{v}.tgz'
    if not os.path.exists(tgz):
        r = subprocess.run(['npm', 'pack', f'{pkg}@{v}', '--pack-destination', D, '--silent'], capture_output=True, text=True)
        name = r.stdout.strip().splitlines()[-1] if r.stdout.strip() else None
        if not name or not os.path.exists(f'{D}/{name}'): return None
        os.rename(f'{D}/{name}', tgz)
    out = set()
    with tarfile.open(tgz) as tf:
        for m in tf.getmembers():
            if m.isfile() and re.search(r'\.(c|m)?js$', m.name) and '.min.' not in m.name and m.size < 6e6:
                txt = tf.extractfile(m).read().decode('utf8', 'replace')
                for g in STR.finditer(txt):
                    s = g.group(1) or g.group(2) or g.group(3)
                    out.add(s)
    return out
cache = f'data/versionscan/{pkg.replace("/", "__")}.lits.json'
os.makedirs('data/versionscan', exist_ok=True)
L = json.load(open(cache)) if os.path.exists(cache) else {}
for v in vs:
    if v not in L:
        s = lits_of_version(v)
        if s is not None: L[v] = sorted(s)
json.dump(L, open(cache, 'w'))
# CC src literals (population region) -> raw-escaped forms as they appear in source is hard; compare on bundle literal text
B = '/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js'
cc = set(json.load(open('data/cc-src-literals.json')))
if os.environ.get('REGION'):
    a, b = map(int, os.environ['REGION'].split(':'))
    seg = open(B).read()[a:b]
    cc = set(g.group(1) or g.group(2) or g.group(3) for g in STR.finditer(seg))
allv = set().union(*[set(x) for x in L.values()]) if L else set()
cand = cc & allv
res = []
for v in vs:
    if v not in L: continue
    s = set(L[v]); res.append((v, len(cand & s), len(s), round(len(cand & s) / max(1, len(s & allv & set().union(*[set(x) for x in L.values()]))), 3)))
best = max(r[1] for r in res) if res else 0
out = {'pkg': pkg, 'candidates': len(cand), 'best': [r[0] for r in res if r[1] == best], 'scores': res}
json.dump(out, open(f'data/versionscan/{pkg.replace("/", "__")}{os.environ.get("TAG","")}.json', 'w'), indent=0)
print(pkg, 'cand', len(cand), 'best', out['best'], 'score', best, 'top5', sorted(res, key=lambda r: -r[1])[:5])
