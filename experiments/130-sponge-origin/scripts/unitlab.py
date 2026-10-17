import json,sys,os,collections
sys.path.insert(0,"/work/exp130-sponge-origin/experiments/096-library-matching-score")
import lib096
corpus,eid,label,s0=sys.argv[1],sys.argv[2],sys.argv[3],int(sys.argv[4])
lib096.use(corpus)
import score,truth_audit
om=lib096.load_origins_manifest(); cm=truth_audit.load_copies_manifest(); chm=truth_audit.load_chained_manifest(); o7m=truth_audit.load_origins7_manifest()
e=[x for x in lib096.load_manifest()["entries"] if x["id"]==eid][0]
truth=truth_audit.patch_sources(e,lib096.load_truth(e),chm)
origins=truth_audit.origin_layer(e,om,o7m)
truth,origins=truth_audit.redecode(e,truth,origins)
rep=json.load(open(os.path.join(lib096.DATA,"results",label,"reports",eid+".json")))["libraryDiscovery"]
units=score.run_units(rep); st=score.statement_units(units)
lab=score.label_units(truth,units,origins,truth_audit.Audit(e,truth,cm))
for u in st:
    if u["s"]==s0: print(u.get("s"),u.get("e"),u["b"],lab[u["i"]]); break
print(truth.keys())
segs=truth["segments"]; srcs=truth["sources"]; offs=[x[0] for x in segs]
import bisect
run_at=[r[0] for r in origins["runs"]]
k=max(bisect.bisect_right(offs,s0)-1,0); e0=u["e"]
c=collections.Counter(); lines=collections.Counter()
while k<len(segs) and segs[k][0]<e0:
    a,b=max(segs[k][0],s0),min(segs[k+1][0] if k+1<len(segs) else e0,e0)
    r=origins["runs"][bisect.bisect_right(run_at,k)-1]
    c[(srcs[segs[k][1]][0], origins["names"][r[1]] if r[1]>=0 else None)]+=b-a
    lines[(srcs[segs[k][1]][2], segs[k][2] if len(segs[k])>2 else None)]+=b-a
    k+=1
print(c); print(sorted(lines.items())[:10], len(lines)); print(segs[k-1])
