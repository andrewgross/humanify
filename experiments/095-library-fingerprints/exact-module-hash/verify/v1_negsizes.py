import sys, json, collections
sys.path.insert(0,'scripts')
from load import load, is_lib
rows=[r for r in load() if r['ok']]
app=[r for r in rows if r['pkg']=='<app>']
print('all <app> spans in lab', len(app))
for cfg in ['bun-1.4.0/min','esbuild-0.28.2/min']:
    a=[r for r in app if r['cfg']==cfg and r['app'].startswith('combo-')]
    ls=sorted(r['normLen'] for r in a)
    print(cfg,'combo app spans',len(a),'normLen>=400:',sum(l>=400 for l in ls),'max',ls[-1] if ls else None, 'bytes total',sum(r['bytes'] for r in a))
    a2=[r for r in app if r['cfg']==cfg]
    print(cfg,'all-lab app spans',len(a2),'>=400:',sum(r['normLen']>=400 for r in a2))
# CC negatives
for v in ['2.1.198','2.1.216']:
    hs=[json.loads(l) for l in open(f'data/cc-{v}-hash.jsonl')]
    for pre in ['negatives:','negative-functions:']:
        n=[h for h in hs if str(h['id']).startswith(pre) and h['ok']]
        ls=sorted(h['normLen'] for h in n)
        print(v,pre,len(n),'normLen>=400:',sum(l>=400 for l in ls),'median',ls[len(ls)//2])
