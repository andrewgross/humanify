"""Claude Code 2.1.216/2.1.198 looked up in the POOLED index (lab Bun-min + fresh 400 packages Bun-min, 1.4.0+1.3.14),
normM, normLen>=400. New hits vs the lab-only index, negatives re-checked, and new hits listed for inspection."""
import json, sys, collections, re
sys.path.insert(0, 'scripts')
from load import load, is_lib
F = 'verify/fresh'
lab = collections.defaultdict(set); labV = set()
for r in load():
    if r['ok'] and is_lib(r) and r['file'] and (r['purity'] is None or r['purity'] >= 0.9):
        lab[r['normM']].add(f"{r['pkg']}/{r['file']}"); labV.add(r['normV'])
fresh = collections.defaultdict(set); freshV = set()
hs = {json.loads(l)['id']: json.loads(l) for l in open(f'{F}/hash.jsonl')}
for i, l in enumerate(open(f'{F}/labels-all.jsonl')):
    r = json.loads(l)
    if r['pkg'].startswith('<') or not r['file'] or (r['purity'] is not None and r['purity'] < 0.9): continue
    h = hs.get(i)
    if h and h['ok']: fresh[h['normM']].add(f"{r['pkg']}/{r['file']}"); freshV.add(h['normV'])
for v in ['2.1.198', '2.1.216']:
    H = {h['id']: h for h in map(json.loads, open(f'data/cc-{v}-hash.jsonl'))}
    mods = [h for k, h in H.items() if str(k).startswith('mod:') and h['ok'] and h['normLen'] >= 400]
    labhit = [h for h in mods if h['normM'] in lab]
    new = [h for h in mods if h['normM'] in fresh and h['normM'] not in lab]
    newV = [h for h in new if h['normV'] in freshV]
    negs = [h for k, h in H.items() if str(k).startswith('negative') and h['ok'] and h['normLen'] >= 400]
    nh = [h for h in negs if h['normM'] in lab or h['normM'] in fresh]
    print(f'{v}: modules>=400 {len(mods)}; lab-index hits {len(labhit)}; NEW hits from the fresh 400 packages {len(new)} (normV agrees {len(newV)}); negatives+negFns hit {len(nh)}/{len(negs)}')
    with open(f'verify/v6_cc_newhits_{v}.txt', 'w') as f:
        for h in sorted(new, key=lambda h: h['normLen']):
            i = int(h['id'].split(':')[1])
            t = open(f'data/cc-{v}/mods/{i:05d}.js').read()
            f.write(f"\n## mod {i} normLen {h['normLen']} normV-agree {h['normV'] in freshV} cands {sorted(fresh[h['normM']])[:4]} (+{max(0,len(fresh[h['normM']])-4)})\n{t[:300]}\n")
