"""exp141: right / wrong bytes of the TERSER-MADE seeds of a holders.py dossier by one feature at a time (scope,
whole-unit hit, holders, context ratio, nodes, rival scores). DEV corpora only.

usage: python3 -I buckets141.py <holders.jsonl>"""
import json,collections,sys,math
RIGHT=("right","family","origin","selfOrigin")
rows=[json.loads(l) for l in open(sys.argv[1])]
rows=[x for x in rows if x['set'].endswith('terser')]
def v(x): return 'R' if x['verdict'] in RIGHT else 'W' if x['verdict']=='wrongFamily' else 'O'
feats={
 'scope': lambda x: x['scope'],
 'whole': lambda x: x['wholeHit'],
 'nholders': lambda x: min(len(x['holders']),5),
 'sole_holder': lambda x: len(x['holders'])==1,
 'ctxratio': lambda x: int(math.log2(max(1,x['holders'].get(x['package'],[0,1])[1])/max(1,x['mb']))) if x['package'] in x['holders'] else 'na',
 'nodes': lambda x: int(math.log2(max(1,x['scores'][x['package']][0]))),
 'kind': lambda x: x.get('kind','?'),
 'had_chosen_any': lambda x: bool(x['scores0']),
 'rival_scores': lambda x: len([p for p in x['scores'] if p!=x['package']])>0,
 'hidden_rival': lambda x: len([p for p in x['holders'] if p not in x['scores']])>0,
}
for name,f in feats.items():
    C=collections.defaultdict(collections.Counter)
    for x in rows:
        C[f(x)][v(x)]+=x['bytes']
    print('==',name)
    for k in sorted(C, key=str):
        c=C[k]; tot=c['R']+c['W']
        print('  ',k,'R',round(c['R']/1024),'W',round(c['W']/1024,1),'P',round(100*c['R']/tot,2) if tot else '-')
