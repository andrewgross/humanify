"""Summarise the Part A census. usage: python3 scripts/census_report.py -> results/census.json + printed tables"""
import json, re, collections
H="/work/exp095-library-fingerprints/immutable-literals/"
def alnum(s): return re.sub(r'[^a-z0-9]','',s.lower())
def names(val,pkg):
    v=alnum(val); base=pkg.split('/')[-1]; scope=pkg.split('/')[0].lstrip('@') if pkg.startswith('@') else None
    cands=[alnum(base)]+([alnum(scope)] if scope else [])
    cands=[c for c in cands if len(c)>=3]
    # strip common suffixes like .js
    return any(c in v for c in cands) or (len(v)>=3 and v in alnum(pkg))
NAMING={'banner!','banner@license','name+version','package.json','user-agent','error-prefix','debug-namespace?','Symbol.for','Symbol()','github-repo','node_modules-path','env-var','x-vendor-header','displayName','PACKAGE_NAME=','aws-serviceId'}
src=[json.loads(l) for l in open(H+'results/census-hits-sources.jsonl')]
allpk=set(); 
for l in open(H+'data/sources-plus.jsonl'): allpk.add(json.loads(l)['pkg'])
by=collections.defaultdict(list)
for h in src: by[h['det'] if not h['det'].startswith('registry:') else 'registry:*'].append(h)
out={'sources':{},'packagesTotal':len(allpk)}
print('packages in corpus',len(allpk))
print(f"{'detector':22} {'files':>6} {'pkgs':>5} {'namesOwnPkg':>12} {'valuesUnique1pkg':>16}  examples")
for det,hs in sorted(by.items(),key=lambda x:-len({(h['pkg']) for h in x[1]})):
    files={(h['pkg'],h['ver'],h['file']) for h in hs}; pk={h['pkg'] for h in hs}
    vp=collections.defaultdict(set)
    for h in hs: vp[h['val']].add(h['pkg'])
    uniq=sum(1 for v,s in vp.items() if len(s)==1)/max(len(vp),1)
    d={'files':len(files),'packages':len(pk),'distinctValues':len(vp),'valuesInOnePackage':round(uniq,3)}
    if det in NAMING:
        ok=sum(1 for h in hs if names(h['val'],h['pkg'])); d['hitsNamingOwnPackage']=f"{ok}/{len(hs)}"; d['precisionNamesPkg']=round(ok/len(hs),3)
        d['packagesSelfNamed']=len({h['pkg'] for h in hs if names(h['val'],h['pkg'])})
    exs=[]; seenp=set()
    for h in hs:
        if h['pkg'] in seenp: continue
        seenp.add(h['pkg']); exs.append(f"{h['pkg']}: {h['val'][:70]}")
        if len(exs)>=8: break
    d['examples']=exs
    out['sources'][det]=d
    print(f"{det:22} {d['files']:6} {d['packages']:5} {d.get('hitsNamingOwnPackage',''):>12} {d['valuesInOnePackage']:16}  {exs[:3]}")
# registry conventions: (package, key, key-signature) recurring >= 8 times
reg=collections.Counter(); regex=collections.defaultdict(list)
for h in src:
    if not h['det'].startswith('registry:'): continue
    sig,val=h['val'].split(' ',1) if ' ' in h['val'] else (h['val'],'')
    k=(h['pkg'],h['det'][9:],sig); reg[k]+=1
    if len(regex[k])<5: regex[k].append(val)
conv=[]
for (p,key,sig),n in reg.most_common():
    if n<8: break
    conv.append({'pkg':p,'key':key,'otherKeys':json.loads(sig),'objects':n,'values':regex[(p,key,sig)]})
seen=set(); print('\nREGISTRY CONVENTIONS (same object shape >= 8 times in one package; top per package+key)')
for c in conv:
    if (c['pkg'],c['key']) in seen: continue
    seen.add((c['pkg'],c['key']))
    if len(seen)<=60: print(f"  {c['pkg']:28} {c['key']:10} x{c['objects']:<5} keys={c['otherKeys']} e.g. {c['values'][:4]}")
out['registryConventions']=conv[:300]
# Claude Code
cc=[json.loads(l) for l in open(H+'results/census-hits-cc.jsonl')]
claims={}
for v in ('2.1.198','2.1.216'):
    rows=[json.loads(l) for l in open(H+f'results/cc-claims-{v}-plus-capvocab-origin.jsonl')]
    u=[r for r in rows if r['kind'] in ('cjs','lazy-esm')]
    for r in u: claims[(v,r['i'])]=r['claim']
out['cc']={}
for v in ('2.1.198','2.1.216'):
    byd=collections.defaultdict(list)
    for h in cc:
        if h['cc']==v: byd[h['det'] if not h['det'].startswith('registry:') else 'registry:*'].append(h)
    print('\nCLAUDE CODE',v)
    out['cc'][v]={}
    for det,hs in sorted(byd.items(),key=lambda x:-len(x[1])):
        units={h['i'] for h in hs}
        cl=[(h, claims.get((v,h['i']))) for h in hs]
        withc=[(h,c) for h,c in cl if c]
        agree=sum(1 for h,c in withc if names(h['val'],c))
        vals=collections.Counter(h['val'] for h in hs)
        d={'units':len(units),'cjsUnits':len({h['i'] for h in hs if h['kind']=='cjs'}),'esmUnits':len({h['i'] for h in hs if h['kind']=='lazy-esm'}),
           'distinctValues':len(vals),'top':[f"{a[:70]} x{b}" for a,b in vals.most_common(10)]}
        if det in NAMING and withc: d['vsIndexClaim']=f"{agree}/{len(withc)} hits in index-claimed units name the claimed package"
        out['cc'][v][det]=d
        print(f"  {det:22} units={d['units']:5} (cjs {d['cjsUnits']}, esm {d['esmUnits']}) {d.get('vsIndexClaim','')}  {d['top'][:4]}")
json.dump(out,open(H+'results/census.json','w'),indent=1)
