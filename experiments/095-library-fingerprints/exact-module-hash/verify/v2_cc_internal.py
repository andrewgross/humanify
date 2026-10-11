# Within one Claude Code bundle: how often do two DIFFERENT modules share the shape key normM at each size?
# Real-code proxy for "is normM unique enough at the floor" without needing labels.
import json, collections, hashlib, sys
for v in ['2.1.198','2.1.216']:
    hs=[json.loads(l) for l in open(f'data/cc-{v}-hash.jsonl')]
    mods=[h for h in hs if str(h['id']).startswith('mod:') and h['ok']]
    by=collections.defaultdict(list)
    for h in mods: by[h['normM']].append(h)
    buckets=[(3,200),(200,400),(400,600),(600,800),(800,1600),(1600,10**9)]
    print(v,'modules',len(mods))
    for lo,hi in buckets:
        ms=[h for h in mods if lo<=h['normLen']<hi]
        shared=[h for h in ms if len(by[h['normM']])>1]
        diffstr=[h for h in shared if len({x['normV'] for x in by[h['normM']]})>1]
        rawdiff=[h for h in shared if len({x['sha'] for x in by[h['normM']]})>1]
        print(f'  normLen {lo}-{hi}: {len(ms)} modules, share shape with another module {len(shared)} ({100*len(shared)/max(1,len(ms)):.1f}%), of which the strings differ {len(diffstr)}')
    # list examples >=400 with differing strings
    seen=set()
    ex=[]
    for k,g in by.items():
        if len(g)>1 and g[0]['normLen']>=400 and len({x['normV'] for x in g})>1:
            ex.append((g[0]['normLen'],len(g),[x['id'] for x in g][:4]))
    ex.sort()
    print('  examples (normLen, count, ids):', ex[:15])
    json.dump(ex, open(f'verify/cc_internal_shape_{v}.json','w'))
