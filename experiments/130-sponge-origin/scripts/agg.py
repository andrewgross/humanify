import json,sys,collections
GOOD={"right","family","origin"}
for f in sys.argv[1:]:
    rows=[json.loads(l) for l in open(f)]
    by=collections.defaultdict(collections.Counter)
    for r in rows:
        va=r.get("a",{}).get("verdict","none"); vb=r.get("b",{}).get("verdict","none")
        ga="good" if va in GOOD else va; gb="good" if vb in GOOD else vb
        by[r["set"]][(ga,gb)]+=r["bytes"]
    for s,c in by.items():
        dw=sum(b for (x,y),b in c.items() if y=="wrongFamily")-sum(b for (x,y),b in c.items() if x=="wrongFamily")
        print(s,"dWrong",dw, {f"{x}->{y}":b for (x,y),b in c.most_common() if x!=y})
