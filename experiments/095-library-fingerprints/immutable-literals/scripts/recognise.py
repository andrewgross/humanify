"""Package recognition from literal sets: precision / recall / leave-package-out false claims.
usage: tools/venv/bin/python scripts/recognise.py [T C M]  -> results/recognise.json + tables"""
import os, json, sys, collections, numpy as np
V = os.environ.get('VARIANT', '')  # weight variant suffix ('' = plain IDF, '-capvocab' = word-like literals capped)
O = V + ('-origin' if os.environ.get('CLAIM') == 'origin' else '')  # output suffix
H="/work/exp095-library-fingerprints/immutable-literals/"
R=[json.loads(l) for l in open(H+'results/scores-lab'+V+'.jsonl')]
CC=[json.loads(l) for l in open(H+'results/scores-cc'+V+'.jsonl')]

def claim_origin(top, topLen, tot, T, C, M, exclude=None, ratio=3.0):
    """Like claim, but a near-tie (within M bits) between packages is broken by SPECIFICITY: the package whose
    best-matching file is the smallest (fewest literals) wins when every other near-tied package's file is at least
    `ratio` times larger -- i.e. the others only hold the code inside a pre-bundled / vendored aggregate."""
    t=[(p,s,l) for (p,s),l in zip(top,topLen) if p!=exclude]
    if not t: return None
    p,s,l=t[0]
    if s<T or s<C*tot: return None
    near=[x for x in t[1:] if s-x[1]<M]
    if not near: return p
    cand=[(p,s,l)]+near
    cand.sort(key=lambda x:x[2])
    best=cand[0]
    if all(x[2]>=ratio*best[2] for x in cand[1:]) and best[1]>=T and best[1]>=C*tot: return best[0]
    return None

CLAIM = os.environ.get('CLAIM', 'plain')  # 'plain' = margin over the runner-up; 'origin' = claim_origin tie-break

def claim_q(q, T, C, M, exclude=None):
    if CLAIM == 'origin': return claim_origin(q['top'], q['topLen'], q['tot'], T, C, M, exclude)
    return claim(q['top'], q['tot'], T, C, M, exclude)

def claim(top, tot, T, C, M, exclude=None):
    t=[x for x in top if x[0]!=exclude]
    if not t: return None
    (p,s),(p2,s2)=t[0],t[1] if len(t)>1 else ('',0)
    if s>=T and s>=C*tot and s-s2>=M: return p
    return None

def evaluate(T,C,M):
    st=collections.Counter(); byb=collections.Counter()
    for r in R:
        lib=not r['pkg'].startswith('<')
        if lib and r['minPurity']<0.9: continue
        c=claim_q(r, T,C,M)
        if lib:
            st['lib']+=1; byb['lib']+=r['bytes']*r['copies']
            if c==r['pkg']: st['tp']+=1; byb['tp']+=r['bytes']*r['copies']
            elif c: st['wrong']+=1
            c2=claim_q(r, T,C,M,exclude=r['pkg'])
            if c2: st['lpoFP']+=1
        elif r['pkg']=='<app>':
            st['app']+=1; st['appFP']+= c is not None
        else:
            st['glue']+=1; st['glueClaim']+= c is not None
    for r in CC:
        if r['set'] in('negatives','negative-functions'):
            st['cc-'+r['set']]+=1; st['cc-'+r['set']+'-FP']+= claim_q(r, T,C,M) is not None
    return dict(T=T,C=C,M=M,recall=round(st['tp']/st['lib'],4),byteRecall=round(byb['tp']/byb['lib'],4),
        precision=round(st['tp']/max(st['tp']+st['wrong'],1),5),wrong=st['wrong'],lpoFP=st['lpoFP'],lpoRate=round(st['lpoFP']/st['lib'],5),
        appFP=f"{st['appFP']}/{st['app']}",glueClaim=f"{st['glueClaim']}/{st['glue']}",
        ccNegFP=f"{st['cc-negatives-FP']}/{st['cc-negatives']}",ccFnFP=f"{st['cc-negative-functions-FP']}/{st['cc-negative-functions']}")
if __name__=='__main__':
    res=[]
    for T in (10,20,40,80):
      for C in (0.5,0.7,0.9):
        for M in (0,10,20):
          e=evaluate(T,C,M); res.append(e); print(e)
    json.dump(res,open(H+'results/recognise-grid'+O+'.json','w'),indent=1)
