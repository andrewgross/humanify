import sys,json
for l in sys.stdin:
    try: r=json.loads(l)
    except: print(l.strip()); continue
    print(r['fam'],r['R'],r['detector'],'| F',r['foreign'],r['foreignNamed'],r['foreignRate'],'B',r['foreignBytesRate'],'copies',r['foreignFamilyCopies'],'| P',r['pos'],r['recall'],'ex',r['recallExactVer'],'nx',r['recallOtherVer'],'wrong',r['wrongPkg'],'| app',r['appGroupsNamed'],r['appFnsNamed'])
