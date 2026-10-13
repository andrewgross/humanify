"""Recall on Claude Code src/ where ground truth exists: lazy-ESM groups whose text carries an MSAL banner
(/*! @azure/msal-common v15.13.1 */, /*! @azure/msal-node v3.8.1 */). Every statement in such a group is MSAL.
usage: VARIANT=... tools/venv/bin/python scripts/cc_msal.py"""
import os, json, sys, collections, bisect, re
sys.path.insert(0,'/work/exp095-library-fingerprints/immutable-literals/scripts')
from fill import propagate
V=os.environ.get('VARIANT','')
O = V + ('-origin' if os.environ.get('CLAIM') == 'origin' else '')
H="/work/exp095-library-fingerprints/immutable-literals/"
T,C,M,G=20,0.9,10,8
CC=[json.loads(l) for l in open(H+f'results/scores-cc{V}.jsonl')]
out={}
for v in ('2.1.198','2.1.216'):
  txt=open(f'/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-{v}/binary-decompiled/src/entrypoints/index.js').read()
  groups=[r for r in CC if r['cc']==v and r['set']=='unit' and r['kind']=='lazy-esm']
  msal=[(g['startChar'],g['endChar'],re.search(r'/\*! @azure/(msal-[a-z]+) v([\d.]+)',txt[g['startChar']:g['endChar']]).group(1)) for g in groups if re.search(r'/\*! @azure/msal-[a-z]+ v',txt[g['startChar']:g['endChar']])]
  ms=[a for a,b,c in msal]
  units=[r for r in CC if r['cc']==v and r['set']=='unit' and r['kind']=='cjs']
  cj=sorted((u['startChar'],u['endChar']) for u in units); cs=[a for a,b in cj]
  def in_cjs(x):
    j=bisect.bisect_right(cs,x)-1; return j>=0 and cj[j][0]<=x<cj[j][1]
  stm=sorted([r for r in CC if r['cc']==v and r['set']=='stmt' and not in_cjs(r['startChar'])],key=lambda r:r['startChar'])
  own,fill,cfill=propagate(stm,T,C,M,G)
  st=collections.Counter()
  for s,o,cf in zip(stm,own,cfill):
    j=bisect.bisect_right(ms,s['startChar'])-1
    if j<0 or not (msal[j][0]<=s['startChar']<msal[j][1]): continue
    B=s['bytes']; st['n']+=1; st['b']+=B
    for tag,x in (('own',o),('cfill',cf)):
      if x and x.startswith('@azure/msal'): st[tag+'_ok']+=B; st[tag+'_okn']+=1
      if x and x==('@azure/'+msal[j][2]): st[tag+'_exact']+=B
      elif x and not x.startswith('@azure/msal'): st[tag+'_other']+=B
  o={'msalGroups':len(msal),'stmts':st['n'],'KB':round(st['b']/1e3),'ownByteRecall(msal-*)':round(st['own_ok']/st['b'],3),'ownExactPkg':round(st['own_exact']/st['b'],3),
     'cfillByteRecall':round(st['cfill_ok']/st['b'],3),'stmtRecall':round(st['own_okn']/st['n'],3),'otherPkgKB':round(st['own_other']/1e3,1)}
  out[v]=o; print(V or 'plain',v,o)
json.dump(out,open(H+f'results/cc-msal{O}.json','w'),indent=1)
