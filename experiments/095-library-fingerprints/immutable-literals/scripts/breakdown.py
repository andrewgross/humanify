"""Recall by bundler config x kind and by span size (literal count); FP by size.  usage: ... breakdown.py [T C M]"""
import os, json, sys, collections
sys.path.insert(0,'/work/exp095-library-fingerprints/immutable-literals/scripts')
from recognise import claim_q, R, CC
V = os.environ.get('VARIANT', '')  # weight variant suffix ('' = plain IDF, '-capvocab' = word-like literals capped)
O = V + ('-origin' if os.environ.get('CLAIM') == 'origin' else '')  # output suffix
H="/work/exp095-library-fingerprints/immutable-literals/"
T,C,M=[float(x) for x in sys.argv[1:4]] if len(sys.argv)>3 else (20,0.9,10)
g=collections.defaultdict(collections.Counter)
def nb(n): return '0' if n==0 else '1-2' if n<=2 else '3-5' if n<=5 else '6-10' if n<=10 else '11-30' if n<=30 else '31-100' if n<=100 else '>100'
for r in R:
  if r['pkg'].startswith('<') or r['minPurity']<0.9: continue
  c=claim_q(r, T,C,M)
  for cf in r['cfgs']:
    for key in ((cf,r['kind']),('ALL',r['kind']),(cf,'ALL'),('ALL','ALL')):
      k=g[key]; k['n']+=1; k['b']+=r['bytes']; k['tp']+=c==r['pkg']; k['tpb']+=r['bytes']*(c==r['pkg']); k['wrong']+=bool(c and c!=r['pkg'])
  k=g[('size',nb(r['n']))]; k['n']+=1; k['b']+=r['bytes']; k['tp']+=c==r['pkg']; k['tpb']+=r['bytes']*(c==r['pkg'])
  k['lpo']+=claim_q(r, T,C,M,exclude=r['pkg']) is not None
for r in R:
  if r['pkg']=='<app>':
    k=g[('appsize',nb(r['n']))]; k['n']+=1; k['fp']+=claim_q(r, T,C,M) is not None
out={}
print('thresholds',T,C,M)
for key in sorted(g):
  k=g[key]
  d={'spans':k['n'],'recall':round(k['tp']/k['n'],3)}
  if k['b']: d['byteRecall']=round(k['tpb']/k['b'],3); d['MB']=round(k['b']/1e6,1)
  if 'lpo' in k: d['lpoRate']=round(k['lpo']/k['n'],4)
  if 'wrong' in k: d['wrong']=k['wrong']
  if 'fp' in k: d['appFP']=k['fp']
  out['/'.join(key)]=d; print(key,d)
json.dump(out,open(H+f'results/breakdown-{int(T)}-{C}-{int(M)}{O}.json','w'),indent=1)
