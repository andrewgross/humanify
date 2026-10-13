"""Step B evaluation of own / fill / consistent-fill on (1) the lab's minified builds (truth = label span) and
(2) Claude Code's src/ statements: the 100 negative groups and every statement that carries a product string.
usage: VARIANT=... tools/venv/bin/python scripts/stepb.py [T C M G] -> results/stepb<V>.json"""
import os, json, sys, collections, bisect, re
sys.path.insert(0,'/work/exp095-library-fingerprints/immutable-literals/scripts')
V = os.environ.get('VARIANT', '')
O = V + ('-origin' if os.environ.get('CLAIM') == 'origin' else '')  # output suffix
from fill import propagate
H="/work/exp095-library-fingerprints/immutable-literals/"
LAB="/work/exp095-library-fingerprints/lab/"
T,C,M,G=[float(x) for x in sys.argv[1:5]] if len(sys.argv)>4 else (20,0.9,10,8)
res={'thresholds':[T,C,M,G]}
S={}
for l in open(H+f'results/scores-stmts{V}.jsonl'):
  r=json.loads(l); S[r['sha']]=r
builds=collections.defaultdict(list)
for l in open(H+'data/stmts.jsonl'):
  if l.strip(): r=json.loads(l); builds[r['build']].append(r)
for scope in ('all','combo','bun-1.4.0'):
  st=collections.Counter()
  for b,rows in builds.items():
    if scope=='combo' and '/combo-' not in b: continue
    if scope=='bun-1.4.0' and not b.startswith('bun-1.4.0/'): continue
    qs=[S[s['sha']] for s in rows]
    own,fill,cfill=propagate(qs,T,C,M,int(G))
    for s,o,f,cf in zip(rows,own,fill,cfill):
      p=s['pkg']; t='app' if p=='<app>' else 'glue' if p.startswith('<') else 'lib'
      if t=='lib' and s['purity'] is not None and s['purity']<0.9: continue
      B=s['bytes']; st[t+'_n']+=1; st[t+'_b']+=B
      for tag,x in (('own',o),('fill',f),('cfill',cf)):
        if not x: continue
        if t=='lib':
          ok=x==p; st[f'{tag}_ok_n']+=ok; st[f'{tag}_ok_b']+=B*ok; st[f'{tag}_bad_n']+=not ok; st[f'{tag}_bad_b']+=B*(not ok)
        elif t=='app': st[f'{tag}_app_n']+=1; st[f'{tag}_app_b']+=B
  o={'libStmts':st['lib_n'],'libMB':round(st['lib_b']/1e6,1),'appStmts':st['app_n']}
  for tag in ('own','fill','cfill'):
    ok,bad=st[f'{tag}_ok_n'],st[f'{tag}_bad_n']
    o[tag]={'stmtRecall':round(ok/st['lib_n'],3),'byteRecall':round(st[f'{tag}_ok_b']/st['lib_b'],3),'wrongPkgStmts':bad,'wrongPkgKB':round(st[f'{tag}_bad_b']/1e3),
            'precision':round(ok/max(ok+bad,1),4),'appStmtsClaimed':st[f'{tag}_app_n']}
  res['lab/'+scope]=o; print('lab',scope,json.dumps(o))
# Claude Code
PROD=re.compile(r'tengu_|CLAUDE_CODE_|claude\.ai|Claude Code|\.claude/|CLAUDE\.md')
for v in ('2.1.198','2.1.216'):
  code=open(f'/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-{v}/binary-decompiled/src/entrypoints/index.js','rb').read()
  txt=code.decode('utf8')
  neg=[json.loads(l) for l in open(f'{LAB}realworld/cc-{v}/negatives.jsonl')]
  rng=sorted((len(code[:n['start']].decode('utf8')), len(code[:n['end']].decode('utf8'))) for n in neg)
  CC=[json.loads(l) for l in open(H+f'results/scores-cc{V}.jsonl')]
  units=[r for r in CC if r['cc']==v and r['set']=='unit' and r['kind']=='cjs']
  cj=sorted((u['startChar'],u['endChar']) for u in units); cs=[a for a,b in cj]
  def in_cjs(x):
    j=bisect.bisect_right(cs,x)-1; return j>=0 and cj[j][0]<=x<cj[j][1]
  stm=sorted([r for r in CC if r['cc']==v and r['set']=='stmt' and not in_cjs(r['startChar'])],key=lambda r:r['startChar'])
  own,fill,cfill=propagate(stm,T,C,M,int(G))
  st=collections.Counter(); ex=collections.defaultdict(list)
  for s,o,f,cf in zip(stm,own,fill,cfill):
    t=txt[s['startChar']:s['endChar']]
    isneg=any(a<=s['startChar']<b for a,b in rng)
    isprod=bool(PROD.search(t))
    B=s['bytes']
    st['src_n']+=1; st['src_b']+=B
    for tag,x in (('own',o),('fill',f),('cfill',cf)):
      if x: st[f'{tag}_src_b']+=B
      for name,flag in (('neg',isneg),('prod',isprod)):
        if not flag: continue
        if tag=='own': st[f'{name}_n']+=1; st[f'{name}_b']+=B
        if x:
          st[f'{tag}_{name}_n']+=1; st[f'{tag}_{name}_b']+=B
          if len(ex[f'{tag}_{name}'])<6: ex[f'{tag}_{name}'].append([x,s['top'][:2],round(s['tot'],1),t[:160]])
  o={'srcStmts':st['src_n'],'srcMB':round(st['src_b']/1e6,2),'negStmts':st['neg_n'],'negKB':round(st['neg_b']/1e3),'prodStmts':st['prod_n'],'prodKB':round(st['prod_b']/1e3)}
  for tag in ('own','fill','cfill'):
    o[tag]={'srcClaimedMB':round(st[f'{tag}_src_b']/1e6,2),'negClaimed':f"{st[f'{tag}_neg_n']} stmts / {round(st[f'{tag}_neg_b']/1e3,1)} KB",
            'prodClaimed':f"{st[f'{tag}_prod_n']} stmts / {round(st[f'{tag}_prod_b']/1e3,1)} KB"}
  o['examples']=ex
  res['cc/'+v]=o; print('cc',v,json.dumps({k:v2 for k,v2 in o.items() if k!='examples'}))
  for k,e in ex.items():
    if k.startswith('own') or k.startswith('cfill'): print(' ',k,e[:4])
json.dump(res,open(H+f'results/stepb{O}.json','w'),indent=1)
