import json,sys
sys.path.insert(0,'scripts')
from load import load
want=[tuple(a.split('::')) for a in sys.argv[1:]]
F='verify/fresh'
hs={json.loads(l)['id']:json.loads(l) for l in open(f'{F}/hash.jsonl')}
found={}
for i,l in enumerate(open(f'{F}/labels-all.jsonl')):
    r=json.loads(l)
    if r['bundlerVersion']=='1.4.0' and (r['pkg'],r['file']) in want and (r['pkg'],r['file']) not in found and hs.get(i,{}).get('ok'):
        found[(r['pkg'],r['file'])]=(open(f"{F}/{r['textPath']}").read(),hs[i])
for r in load():
    if r['cfg']=='bun-1.4.0/min' and (r['pkg'],r['file']) in want and (r['pkg'],r['file']) not in found and r['ok']:
        found[(r['pkg'],r['file'])]=(open('/work/exp095-library-fingerprints/lab/'+r['textPath']).read(),{'normM':r['normM'],'normV':r['normV'],'normLen':r['normLen']})
for k in want:
    t,h=found.get(k,('MISSING',{}))
    print('==',k,'normM',h.get('normM'),'normV',h.get('normV'),'len',h.get('normLen'))
    print(t[:700])
