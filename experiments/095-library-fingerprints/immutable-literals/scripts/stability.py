"""Literal stability: what share of a minified span's literals appear in its own npm source file.
usage: tools/venv/bin/python scripts/stability.py  -> results/stability.json + printed table"""
import os, json, collections, numpy as np
V = os.environ.get('VARIANT', '')  # weight variant suffix ('' = plain IDF, '-capvocab' = word-like literals capped)
H="/work/exp095-library-fingerprints/immutable-literals/"
R=[json.loads(l) for l in open(H+'results/scores-lab'+V+'.jsonl')]
lib=[r for r in R if not r['pkg'].startswith('<') and r['minPurity']>=0.9 and 'cont' in r]
out={}
def q(a,ps=(5,10,25,50)): return [round(float(np.percentile(a,p)),3) for p in ps]
g=collections.defaultdict(list); gn=collections.defaultdict(list); gc=collections.defaultdict(list)
for r in lib:
  for cf in r['cfgs']:
    g[(cf,r['kind'])].append(r['cont'] if r['n'] else np.nan); gn[(cf,r['kind'])].append(r['n']); gc[(cf,r['kind'])].append(r['srcCover'])
print(f"{'config':16} {'kind':10} {'spans':>6} {'no-lits':>8} {'cont p5/p10/p25/p50 (spans with lits)':>40} {'all-in-src':>10} {'srcCover p50':>12}")
for k in sorted(g):
  a=np.array(g[k]); has=a[~np.isnan(a)]
  row=dict(spans=len(a), noLits=int(np.isnan(a).sum()), cont=q(has), allIn=round(float(np.mean(has>=0.999)),3), srcCoverP50=float(np.median(gc[k])), medianLits=float(np.median(gn[k])))
  out['%s/%s'%k]=row
  print(f"{k[0]:16} {k[1]:10} {row['spans']:6} {row['noLits']:8} {str(row['cont']):>40} {row['allIn']:10} {row['srcCoverP50']:12}")
json.dump(out,open(H+'results/stability.json','w'),indent=1)
