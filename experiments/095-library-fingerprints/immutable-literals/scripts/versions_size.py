"""(1) version identification and leave-version-out recall, (2) literal-set drift between versions of one file,
(3) index size.  usage: tools/venv/bin/python scripts/versions_size.py -> results/versions.json, results/index-size.json"""
import json, sys, collections, itertools, numpy as np
sys.path.insert(0,'/work/exp095-library-fingerprints/immutable-literals/scripts')
from recognise import claim_q, R
H="/work/exp095-library-fingerprints/immutable-literals/"
LAB="/work/exp095-library-fingerprints/lab/"
T,C,M=20,0.9,10
st=collections.Counter()
for r in R:
  if r['pkg'].startswith('<') or r['minPurity']<0.9: continue
  if claim_q(r, T,C,M)!=r['pkg']: continue
  st['claimed']+=1
  vs=r['bestVers']
  if r['ver'] in vs:
    st['verIn']+=1
    if len(vs)==1: st['verUnique']+=1
  else: st['verWrong']+=1
  # leave-version-out: true package's best score from OTHER versions vs the best other package
  so=r.get('sameOtherVerS')
  if so is None: st['onlyVersion']+=1; continue
  st['lvoN']+=1
  others=[x for x in r['top'] if x[0]!=r['pkg']]
  s2=others[0][1] if others else 0
  if so>=T and so>=C*r['tot'] and so-s2>=M: st['lvoClaim']+=1
  elif others and others[0][1]>=T and others[0][1]>=C*r['tot'] and others[0][1]-so>=M: st['lvoWrongPkg']+=1
print(dict(st))
# version distance of drift: same file in two versions of a package, Jaccard of source literal sets
vers=json.load(open(LAB+'versions.json'))
roles={}
for p in vers:
  for role,v in p['versions'].items(): roles[(p['name'],v)]=role
S=collections.defaultdict(dict)
for l in open(H+'data/sources.jsonl'):
  x=json.loads(l)
  if (x['pkg'],x['ver']) in roles and x['lits']: S[(x['pkg'],x['file'])][x['ver']]=set(x['lits'])
dr=collections.defaultdict(list)
for (p,f),vv in S.items():
  lat=[v for v in vv if roles[(p,v)]=='latest']
  if not lat: continue
  a=vv[lat[0]]
  for v,b in vv.items():
    if v==lat[0]: continue
    dr[roles[(p,v)]].append((len(a&b)/len(a|b), len(a&b)/len(b)))
drift={}
for k,v in dr.items():
  j=np.array([x[0] for x in v]); c=np.array([x[1] for x in v])
  drift[k]={'files':len(v),'jaccardP10/50':[round(float(np.percentile(j,10)),3),round(float(np.median(j)),3)],'identical':round(float(np.mean(j>=0.999)),3),'oldInNewP10/50':[round(float(np.percentile(c,10)),3),round(float(np.median(c)),3)]}
print(json.dumps(drift,indent=1))
json.dump({'thresholds':[T,C,M],'stats':dict(st),'drift_vs_latest':drift},open(H+'results/versions.json','w'),indent=1)

# index size
files=0; post=0; vocab=set(); pv=collections.defaultdict(set); pk=collections.defaultdict(set); dfp=collections.defaultdict(set)
for l in open(H+'data/sources.jsonl'):
  x=json.loads(l); files+=1; post+=len(x['lits']); vocab.update(x['lits'])
  pv[(x['pkg'],x['ver'])].update(x['lits']); pk[x['pkg']].update(x['lits'])
  for w in x['lits']: dfp[w].add(x['pkg'])
sz={'files':files,'filePostings':post,'distinctLiterals':len(vocab),'pkgVersions':len(pv),'pkgVersionPostings':sum(map(len,pv.values())),
    'packages':len(pk),'packagePostings(all versions merged)':sum(map(len,pk.values()))}
for D in (1,2,3,5,10):
  sz[f'pkgPostings_df<={D}']=sum(1 for w,s in dfp.items() if len(s)<=D for _ in s)
pvs=np.array([len(s) for s in pv.values()])
sz['litsPerPkgVersion p50/p90/max']=[int(np.median(pvs)),int(np.percentile(pvs,90)),int(pvs.max())]
print(json.dumps(sz,indent=1))
json.dump(sz,open(H+'results/index-size.json','w'),indent=1)
