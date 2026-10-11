"""Claude Code full scan: every CJS factory (vendor/) and every lazy-ESM group and top-level statement (src/).
usage: tools/venv/bin/python scripts/cc_scan.py [T C M] -> results/cc-scan.json, results/cc-claims-<v>.jsonl"""
import os, json, sys, collections, bisect
sys.path.insert(0,'/work/exp095-library-fingerprints/immutable-literals/scripts')
from recognise import claim_q
V = os.environ.get('VARIANT', '')  # weight variant suffix ('' = plain IDF, '-capvocab' = word-like literals capped)
O = V + ('-origin' if os.environ.get('CLAIM') == 'origin' else '')  # output suffix
H="/work/exp095-library-fingerprints/immutable-literals/"
T,C,M=[float(x) for x in sys.argv[1:4]] if len(sys.argv)>3 else (20,0.9,10)
CC=[json.loads(l) for l in open(H+'results/scores-cc'+V+'.jsonl')]
out={}
for v in ('2.1.198','2.1.216'):
  man=json.load(open(f'/tmp/eval-work/candidate-e214db7a-scratch/{v}/vendor/_bun-modules.json'))['factories']
  units=[r for r in CC if r['cc']==v and r['set']=='unit']
  stmts=sorted([r for r in CC if r['cc']==v and r['set']=='stmt'],key=lambda r:r['startChar'])
  st=collections.Counter(); byp=collections.defaultdict(collections.Counter)
  claims=open(H+f'results/cc-claims-{v}{O}.jsonl','w')
  k=0; agree=collections.Counter(); dis=[]
  for u in units:
    c=claim_q(u, T,C,M)
    if u['kind']=='cjs':
      m=man[k] if k<len(man) else {}; k+=1
      st['cjs_n']+=1; st['cjs_b']+=u['bytes']
      if c:
        st['cjs_claim']+=1; st['cjs_claim_b']+=u['bytes']; byp['cjs'][c]+=u['bytes']
        nm=m.get('name',''); last=c.split('/')[-1]
        a='same' if nm in (c,last,c.replace('/','__')) or last in m.get('fileName','') else 'differs'
        agree[(a,m.get('nameSource'))]+=1
        if a=='differs': dis.append((c,m.get('fileName'),m.get('nameSource'),u['top'][:2],round(u['tot'],1)))
      claims.write(json.dumps({'kind':'cjs','i':u['i'],'claim':c,'manifest':m.get('fileName'),'nameSource':m.get('nameSource'),'top':u['top'][:3],'tot':u['tot'],'bytes':u['bytes'],'n':u['n']})+'\n')
    else:
      st['esm_n']+=1; st['esm_b']+=u['bytes']
      if c: st['esm_claim']+=1; st['esm_claim_b']+=u['bytes']; byp['esm-group'][c]+=u['bytes']
      claims.write(json.dumps({'kind':'lazy-esm','i':u['i'],'claim':c,'top':u['top'][:3],'tot':u['tot'],'bytes':u['bytes'],'n':u['n'],'startChar':u['startChar'],'endChar':u['endChar']})+'\n')
  # statements outside CJS factories (= what lands in src/): own claim + both-sides fill
  cjs_ranges=sorted((u['startChar'],u['endChar']) for u in units if u['kind']=='cjs')
  starts=[a for a,b in cjs_ranges]
  def in_cjs(x):
    j=bisect.bisect_right(starts,x)-1
    return j>=0 and cjs_ranges[j][0]<=x<cjs_ranges[j][1]
  S=[s for s in stmts if not in_cjs(s['startChar'])]
  lab=[claim_q(s, T,C,M) for s in S]
  left=[None]*len(S); right=[None]*len(S); last=None
  for i,c in enumerate(lab): left[i]=last; last=c or last
  last=None
  for i in range(len(S)-1,-1,-1): right[i]=last; last=lab[i] or last
  for i,s in enumerate(S):
    st['src_stmt_n']+=1; st['src_stmt_b']+=s['bytes']
    if lab[i]: st['src_own_n']+=1; st['src_own_b']+=s['bytes']; byp['src-own'][lab[i]]+=s['bytes']
    f=lab[i] or (left[i] if left[i] and left[i]==right[i] else None)
    if f: st['src_fill_b']+=s['bytes']; byp['src-fill'][f]+=s['bytes']
    claims.write(json.dumps({'kind':'src-stmt','startChar':s['startChar'],'endChar':s['endChar'],'claim':lab[i],'fill':f if not lab[i] else None,'top':s['top'][:3],'tot':s['tot'],'bytes':s['bytes'],'n':s['n']})+'\n')
  claims.close()
  o={'stats':dict(st),'cjsAgreement':{f'{a}/{b}':n for (a,b),n in agree.items()},'cjsDisagreeExamples':dis[:40],
     'bytesByPkg':{k:dict(v.most_common(40)) for k,v in byp.items()}}
  out[v]=o
  print(v,json.dumps(o['stats']),json.dumps(o['cjsAgreement']))
  for k2,v2 in byp.items(): print(' ',k2,[(p,round(b/1e3)) for p,b in v2.most_common(25)])
json.dump(out,open(H+'results/cc-scan'+O+'.json','w'),indent=1)
