"""(1) cross-bundler literal-set identity for the same module occurrence, (2) EXACT literal-set hash matching
(sorted literal set -> hash; a span matches a source file only if the sets are identical), (3) index size with
per-file entries deduplicated across versions.  usage: tools/venv/bin/python scripts/extra.py -> results/extra.json"""
import json, collections, hashlib
H="/work/exp095-library-fingerprints/immutable-literals/"; LAB="/work/exp095-library-fingerprints/lab/"
lits={}
for l in open(H+'data/spans.jsonl'):
    r=json.loads(l); lits[r['sha']]=frozenset(r['lits'])
rows=[json.loads(l) for l in open(LAB+'labels/all.jsonl')]
lib=[r for r in rows if r['minified'] and not r['pkg'].startswith('<') and (r['purity'] is None or r['purity']>=0.9)]
occ=collections.defaultdict(dict)
for r in lib:
    occ[(r['app'],r['mode'],r['pkg'],r['pkgVersion'],r['file'],r['part'])][f"{r['bundler']}-{r['bundlerVersion']}"]=r
out={}
pairs=[('bun-1.4.0','bun-1.4.3'),('bun-1.3.14','bun-1.4.0'),('bun-1.4.0','esbuild-0.28.2'),('bun-1.4.0','rollup-4.64.5')]
cb={}
for a,b in pairs:
    n=same_text=same_set=0; jac=[]
    for k,d in occ.items():
        if a in d and b in d:
            ra,rb=d[a],d[b]; n+=1
            same_text+=ra['sha256']==rb['sha256']
            A,B=lits[ra['sha256']],lits[rb['sha256']]
            same_set+=A==B
            if A|B: jac.append(len(A&B)/len(A|B))
    jac.sort()
    cb[f'{a} vs {b}']={'occurrences':n,'identicalText':same_text,'identicalLiteralSet':same_set,'identicalSetShare':round(same_set/n,3),'jaccardP10':round(jac[len(jac)//10],3),'jaccardMedian':round(jac[len(jac)//2],3)}
    print(a,b,cb[f'{a} vs {b}'])
out['crossBundler']=cb
# exact set hashes
def h(s): return hashlib.sha1('\n'.join(sorted(s)).encode('utf-8','surrogatepass')).hexdigest()[:16]
src={}; hp=collections.defaultdict(set)
for l in open(H+'data/sources.jsonl'):
    x=json.loads(l); s=frozenset(x['lits'])
    if not s: continue
    k=h(s); src[(x['pkg'],x['ver'],x['file'])]=k; hp[k].add(x['pkg'])
st=collections.Counter(); bysize=collections.defaultdict(collections.Counter)
seen=set()
for r in lib:
    if r['sha256'] in seen: continue
    seen.add(r['sha256'])
    s=lits[r['sha256']]
    if not s: continue
    k=h(s); n=len(s); b='1-2' if n<=2 else '3-5' if n<=5 else '6-10' if n<=10 else '11-30' if n<=30 else '>30'
    st['spans']+=1; bysize[b]['spans']+=1
    if k in hp:
        st['hashHit']+=1; bysize[b]['hit']+=1
        if r['pkg'] in hp[k]:
            if len(hp[k])==1: st['hitRightUnique']+=1; bysize[b]['right']+=1
            else: st['hitAmbiguous']+=1; bysize[b]['ambig']+=1
        else: st['hitWrongPkg']+=1; bysize[b]['wrong']+=1
    st['ownFileSame']+= src.get((r['pkg'],r['pkgVersion'],r['file']))==k
out['exactSetHash']={'all':dict(st),'bySize':{k:dict(v) for k,v in bysize.items()}}
print(out['exactSetHash'])
# index size, per-file entries deduped across versions of a package
dedup=set(); post=0
for l in open(H+'data/sources.jsonl'):
    x=json.loads(l)
    k=(x['pkg'],x['file'],h(frozenset(x['lits'])))
    if k in dedup: continue
    dedup.add(k); post+=len(x['lits'])
out['indexDedupFiles']={'fileEntries':len(dedup),'postings':post}
print(out['indexDedupFiles'])
json.dump(out,open(H+'results/extra.json','w'),indent=1)
