"""REAL-WORLD: look up every Claude Code module (raw bundle, Bun 1.4.0) in the lab's index.
Reports hits per variant / index group / size floor, what they are, whether the strings agree too
(normV), whether any NEGATIVE (known Claude Code app code) hits, and the positives' recall.
Output: results/cc_match.json + results/cc_hits-<v>.tsv"""
import json, collections, sys
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib, LAB, HERE

rows = [r for r in load() if r['ok'] and is_lib(r) and r['file'] and (r['purity'] is None or r['purity'] >= 0.9)]
GROUPS = {
    'bun-1.4.0/min': lambda c: c == 'bun-1.4.0/min',
    'bun all min': lambda c: c.startswith('bun') and c.endswith('/min'),
    'all 10 configs': lambda c: True,
}
VARS = ['raw', 'coreM', 'normM', 'normV']
FLOORS = [3, 200, 400, 800]  # minimum canonical-stream length of the span's core (3 = non-empty)
LENKEY = {'raw': 'normLen', 'coreM': 'coreLen', 'normM': 'normLen', 'normV': 'normLen'}
index = {}
for g, pred in GROUPS.items():
    for v in VARS:
        d = collections.defaultdict(set)
        for r in rows:
            if pred(r['cfg']):
                d[r[v]].add((r['pkg'], r['ver'], r['file']))
        index[(g, v)] = d

out = {}
for ver in ['2.1.198', '2.1.216']:
    lay = json.load(open(f'{LAB}/realworld/layout-{ver}.json'))['modules']
    hs = {}
    for l in open(f'{HERE}/data/cc-{ver}-hash.jsonl'):
        h = json.loads(l)
        hs[h['id']] = h
    vend = json.load(open(f'/tmp/eval-work/candidate-e214db7a-scratch/{ver}/vendor/_bun-modules.json'))['factories']
    vend_by_hash = {f['structuralHash']: f['fileName'] for f in vend}
    pos = [json.loads(l) for l in open(f'{LAB}/realworld/cc-{ver}/positives.jsonl')]
    res = {}
    print(f'\n######## Claude Code {ver}: {len(lay)} modules '
          f"({sum(m['kind']=='cjs' for m in lay)} CJS, {sum(m['kind']=='lazy-esm' for m in lay)} lazy ESM)")
    print(f"{'index':16s} {'var':6s} {'minLen':>6s} {'CJS hits':>9s} {'ESM hits':>9s} {'hit bytes':>10s} {'strings agree':>13s} {'neg hits':>8s} {'negFn hits':>10s} {'pos recall':>10s}")
    for (g, v), idx in index.items():
        for floor in FLOORS:
            hit = collections.Counter()
            hit_bytes = 0
            agree = 0
            for i, m in enumerate(lay):
                h = hs.get(f'mod:{i}')
                if not h or not h['ok'] or h[LENKEY[v]] < floor:
                    continue
                key = h['sha'] if v == 'raw' else h[v]
                if key in idx:
                    hit[m['kind']] += 1
                    hit_bytes += m['end'] - m['start']
                    if v != 'normV' and h['normV'] in index[(g, 'normV')]:
                        agree += 1
            negs = [h for k, h in hs.items() if k.startswith('negatives:')]
            negf = [h for k, h in hs.items() if k.startswith('negative-functions:')]
            nb = {k: json.loads(l)['bytes'] for k, l in
                  [(f'negatives:{i}', l) for i, l in enumerate(open(f'{LAB}/realworld/cc-{ver}/negatives.jsonl'))]}
            fb = {k: json.loads(l)['bytes'] for k, l in
                  [(f'negative-functions:{i}', l) for i, l in enumerate(open(f'{LAB}/realworld/cc-{ver}/negative-functions.jsonl'))]}
            nh = sum(1 for k, h in hs.items() if k in nb and h['ok'] and h[LENKEY[v]] >= floor and h.get(v) in idx)
            fh = sum(1 for k, h in hs.items() if k in fb and h['ok'] and h[LENKEY[v]] >= floor and h.get(v) in idx)
            ph = sum(1 for i, p in enumerate(pos) if hs[f'positives:{i}']['ok'] and hs[f'positives:{i}'].get(v) in idx)
            res[f'{g}|{v}|{floor}'] = {'cjsHits': hit['cjs'], 'esmHits': hit['lazy-esm'], 'hitBytes': hit_bytes,
                                       'stringsAgree': agree, 'negHits': nh, 'negFnHits': fh, 'posHits': ph, 'pos': len(pos)}
            if v == 'raw' and floor > 3:
                continue
            print(f"{g:16s} {v:6s} {floor:6d} {hit['cjs']:9d} {hit['lazy-esm']:9d} {hit_bytes:10d} {agree:13d} {nh:4d}/{len(negs)} {fh:5d}/{len(negf)} {ph:5d}/{len(pos)}")
    out[ver] = res
    # the hit list for the main setting
    idx = index[('all 10 configs', 'normM')]
    with open(f'{HERE}/results/cc_hits-{ver}.tsv', 'w') as f:
        f.write('i\tkind\tbytes\tnormLen\tstringsAgree\tvendorFile\tcandidates\n')
        for i, m in enumerate(lay):
            h = hs.get(f'mod:{i}')
            if not h or not h['ok'] or h['normM'] not in idx or h['normLen'] < 3:
                continue
            c = sorted(idx[h['normM']])
            names = sorted({f'{p}/{fl}' for p, _, fl in c})
            vf = vend_by_hash.get(h.get('factM')) or ''
            f.write(f"{i}\t{m['kind']}\t{m['end']-m['start']}\t{h['normLen']}\t{h['normV'] in index[('all 10 configs','normV')]}\t{vf}\t"
                    f"{'; '.join(names[:4])}{' (+%d)' % (len(names)-4) if len(names) > 4 else ''} [{len({(p, vv) for p, vv, _ in c})} pkg@ver]\n")
json.dump(out, open(f'{HERE}/results/cc_match.json', 'w'), indent=1)
