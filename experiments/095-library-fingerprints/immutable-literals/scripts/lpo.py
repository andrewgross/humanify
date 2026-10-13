"""Leave-package-out claims (what the index answers when the true package is NOT indexed), classified:
  copy       the claimed file holds >= 80% of the true source file's literals (a vendored / pre-bundled / duplicated copy:
             the code really is in the claimed package)
  look-alike otherwise (different code that shares the query's literals)
usage: tools/venv/bin/python scripts/lpo.py [T C M]"""
import os, json, sys, collections
sys.path.insert(0,'/work/exp095-library-fingerprints/immutable-literals/scripts')
from recognise import claim_q, R
V = os.environ.get('VARIANT', '')  # weight variant suffix ('' = plain IDF, '-capvocab' = word-like literals capped)
O = V + ('-origin' if os.environ.get('CLAIM') == 'origin' else '')  # output suffix
H="/work/exp095-library-fingerprints/immutable-literals/"
T,C,M=[float(x) for x in sys.argv[1:4]] if len(sys.argv)>3 else (20,0.9,10)
cl=[]
for r in R:
  if r['pkg'].startswith('<') or r['minPurity']<0.9: continue
  c=claim_q(r, T,C,M,exclude=r['pkg'])
  if c: cl.append(r)
need=set()
for r in cl: need.add((r['pkg'],r['ver'],r['file'])); v,f=r['otherFile'].split('/',1); need.add((r['otherPkg'],v,f))
S={}
for l in open(H+('data/sources-plus.jsonl' if 'plus' in V else 'data/sources.jsonl')):
  x=json.loads(l); k=(x['pkg'],x['ver'],x['file'])
  if k in need: S[k]=set(x['lits'])
cat=collections.Counter(); pairs=collections.defaultdict(collections.Counter); ex={}
for r in cl:
  v,f=r['otherFile'].split('/',1)
  a=S[(r['pkg'],r['ver'],r['file'])]; b=S[(r['otherPkg'],v,f)]
  share=len(a&b)/max(len(a),1)
  k='copy' if share>=0.8 else 'look-alike'
  cat[k]+=1; pairs[k][(r['pkg'],r['otherPkg'])]+=1
  ex.setdefault((k,r['pkg'],r['otherPkg']),(r['file'],r['otherFile'],round(share,2),r['n'],r['tot'],r['otherS']))
lib=sum(1 for r in R if not r['pkg'].startswith('<') and r['minPurity']>=0.9)
print('thresholds',T,C,M,'library queries',lib,'LPO claims',len(cl),dict(cat))
res={'T':T,'C':C,'M':M,'queries':lib,'claims':len(cl),'cat':dict(cat),'pairs':{k:[[a,b,n,ex[(k,a,b)]] for (a,b),n in v.most_common()] for k,v in pairs.items()}}
for k in pairs:
  print('==',k)
  for (a,b),n in pairs[k].most_common(25): print(n,a,'->',b,ex[(k,a,b)])
json.dump(res,open(H+f'results/lpo-{int(T)}-{C}-{int(M)}{O}.json','w'),indent=1)
