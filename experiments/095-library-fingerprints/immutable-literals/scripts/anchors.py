"""Rare-literal anchors: a literal seen in exactly ONE indexed package. Claim package P when a span holds >= K
anchors of P and no anchor of another package. 'distinctive' anchors exclude identifier-like words.
Leave-package-out: anchors recomputed with the span's own package removed (a literal shared by P and Q becomes an
anchor of Q).  usage: tools/venv/bin/python scripts/anchors.py -> results/anchors.json"""
import json, re, collections
H="/work/exp095-library-fingerprints/immutable-literals/"; LAB="/work/exp095-library-fingerprints/lab/"
WORDY=re.compile(r"^[SPN]:[A-Za-z_$0-9][A-Za-z0-9_$-]*$")
pk=collections.defaultdict(set)
for l in open(H+'data/sources.jsonl'):
    x=json.loads(l)
    for w in x['lits']: pk[w].add(x['pkg'])
lits={}
for l in open(H+'data/spans.jsonl'):
    r=json.loads(l); lits[r['sha']]=r['lits']
rows=[json.loads(l) for l in open(LAB+'labels/all.jsonl')]
q={}
for r in rows:
    if r['minified'] and (r['purity'] is None or r['purity']>=0.9): q.setdefault(r['sha256'],r)
cc=[json.loads(l) for l in open(H+'data/cc.jsonl')]
def decide(L, K, distinct, drop=None):
    cnt=collections.Counter()
    for w in L:
        s=pk.get(w)
        if not s: continue
        if drop: s=s-{drop}
        if len(s)!=1: continue
        if distinct and (WORDY.match(w) or len(w)<8): continue
        cnt[next(iter(s))]+=1
    if not cnt: return None
    (p,n),*rest=cnt.most_common()
    return p if n>=K and not rest else None
res={}
for distinct in (False,True):
    for K in (1,2,3,5):
        st=collections.Counter()
        for sha,r in q.items():
            L=lits[sha]
            if r['pkg'].startswith('<'):
                if r['pkg']=='<app>': st['app']+=1; st['appFP']+=decide(L,K,distinct) is not None
                continue
            st['lib']+=1; st['libB']+=r['bytes']
            c=decide(L,K,distinct)
            if c==r['pkg']: st['ok']+=1; st['okB']+=r['bytes']
            elif c: st['wrong']+=1
            st['lpo']+=decide(L,K,distinct,drop=r['pkg']) is not None
        for x in cc:
            if x['set'] in ('negatives','negative-functions'):
                st['ccneg']+=1; st['ccnegFP']+=decide(x['lits'],K,distinct) is not None
        d={'recall':round(st['ok']/st['lib'],3),'byteRecall':round(st['okB']/st['libB'],3),'wrong':st['wrong'],'precision':round(st['ok']/max(st['ok']+st['wrong'],1),4),
           'lpoRate':round(st['lpo']/st['lib'],4),'appFP':f"{st['appFP']}/{st['app']}",'ccNegFP':f"{st['ccnegFP']}/{st['ccneg']}"}
        res[f"K={K} distinctive={distinct}"]=d; print(K,distinct,d)
json.dump(res,open(H+'results/anchors.json','w'),indent=1)
