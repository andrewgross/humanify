"""exp147: every simulated guard on shown's changed units (feat.py / kepthash.py / ixfiles.py outputs in /work/exp147-data),
one table: per dev set, % of shown's gained-right bytes the guard keeps and new-wrong KB it removes. Family-level
guards drop every changed byte of the (bundle, family); seed-level guards drop the refused seed's module's changes.
No propagation is re-modelled. usage: python3 simall.py [data dir]"""
import collections, json, sys
D = sys.argv[1] if len(sys.argv) > 1 else '/work/exp147-data'
F = json.load(open(f'{D}/feat.json')); K = json.load(open(f'{D}/kepthash.json')); X = json.load(open(f'{D}/ixfiles.json'))
SETS = ['heldout', 'esbuild-app', 'wrangler', 'apps-esbuild', 'apps-rollup', 'apps-webpack', 'dev-rollup', 'pop-bun',
        'pop-esbuild', 'remin-esbuild', 'remin-terser', 'remin3-esbuild', 'remin3-terser', 'remin4-esbuild', 'remin4-terser']
def setof(e):
    for p in ('heldout', 'wrangler'):
        if e.startswith(p): return p
    return {'esbuild-task': 'esbuild-app', 'rollup-dev105': 'dev-rollup'}.get('-'.join(e.split('-')[:2]), '-'.join(e.split('-')[:2]))
def cdir(paths):
    out = []
    for xs in zip(*[p.split('/')[:-1] for p in paths]):
        if all(x == xs[0] for x in xs): out.append(xs[0])
        else: break
    return out
files = collections.defaultdict(lambda: collections.defaultdict(set)); slice_of = {}
for k in K:
    if k['slice'] not in X: continue
    key = (k['entry'], k['fam']); slice_of[key] = k['slice']
    files[key][k['side']].update(f for o, f in X[k['slice']]['rows'].get(k['hash'] or '', []) if o == k['fam'])
def minority_subtree(f, sides):
    key = (f['entry'], f['fam']); allf = X.get(slice_of.get(key), {}).get('files', {}).get(f['fam'])
    fs = set().union(*(files[key][s] for s in sides))
    if not allf or not fs: return False
    d = '/'.join(cdir(sorted(fs)))
    return bool(d) and sum(1 for x in allf if x.startswith(d + '/')) < 0.5 * len(allf)
def any_subtree(f, sides):
    key = (f['entry'], f['fam']); allf = X.get(slice_of.get(key), {}).get('files', {}).get(f['fam'])
    fs = set().union(*(files[key][s] for s in sides))
    return bool(allf and fs) and len(cdir(sorted(fs))) > len(cdir(allf))
FAM = {  # True = refuse the family's rescue in that bundle
    'A shown needs a kept seed of the exact package': lambda f: f['kept'] == 0,
    'B shown needs a whole module the family holds (seed in it)': lambda f: not (f['fullSeedMods'] or f['scopeDecl']),
    'C kept seeds all in one child dir of the code root': lambda f: any_subtree(f, ['kept']),
    'D C, and that dir holds < half the family\'s indexed files': lambda f: minority_subtree(f, ['kept']),
    'E D on kept + rescued seeds': lambda f: minority_subtree(f, ['kept', 'new']),
}
SEED = {
    'F rescued seed\'s module wholly the family\'s after fill': lambda r: r['modCov1'] < 0.999,
    'G rescued seed\'s module >= half the family\'s after fill': lambda r: r['modCov1'] < 0.5,
}
def run(fam=None, seed=None):
    agg = collections.defaultdict(collections.Counter)
    for f in F:
        s = setof(f['entry']); k = f['kinds']
        agg[s]['gr'] += k.get('gainedRight', 0); agg[s]['nw'] += k.get('newWrong', 0)
        if not f['rescued']: continue
        if fam:
            if fam(f): agg[s]['lost'] += k.get('gainedRight', 0); agg[s]['rm'] += k.get('newWrong', 0)
            continue
        bad = [r for r in f['rescued'] if seed(r)]
        if not bad: continue
        if len(bad) == len(f['rescued']):
            agg[s]['lost'] += k.get('gainedRight', 0); agg[s]['rm'] += k.get('newWrong', 0)
        else:
            for m in {str(r['mod']) for r in bad}:
                agg[s]['lost'] += f['mods'].get(m, {}).get('gainedRight', 0); agg[s]['rm'] += f['mods'].get(m, {}).get('newWrong', 0)
    return agg
print('| guard | ' + ' | '.join(SETS) + ' | min kept | wrong removed (of total) |')
print('|---|' + '---:|' * (len(SETS) + 2))
base = run(fam=lambda f: False)
print('| shown: gained right KB / new wrong KB | ' + ' | '.join(f"{base[s]['gr']/1e3:,.0f} / {base[s]['nw']/1e3:.2f}" for s in SETS) + ' | - | - |')
for name, g in list(FAM.items()) + list(SEED.items()):
    a = run(fam=g) if name in FAM else run(seed=g)
    kept = {s: 100 * (1 - a[s]['lost'] / max(1, a[s]['gr'])) for s in SETS}
    rm = sum(a[s]['rm'] for s in SETS); tot = sum(a[s]['nw'] for s in SETS)
    print(f'| {name} | ' + ' | '.join(f"{kept[s]:.1f}%" + (f" (-{a[s]['rm']/1e3:.2f})" if a[s]['rm'] >= 50 else '') for s in SETS)
          + f" | {min(kept.values()):.1f}% | {rm/1e3:.1f} of {tot/1e3:.1f} KB |")
