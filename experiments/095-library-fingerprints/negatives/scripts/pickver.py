"""pick a version per package: declared (from CC readers) else the newest non-prerelease published before CC 2.1.216 (2026-07-20)."""
import json, subprocess, sys, re
from concurrent.futures import ThreadPoolExecutor
CUT = '2026-07-20T20:19:37'
DECLARED = {
 '@anthropic-ai/sdk':'0.94.0','@azure/msal-node':'3.8.1','@azure/msal-common':'15.13.1','axios':'1.15.2',
 '@aws-sdk/client-bedrock':'3.936.0','@aws-sdk/client-bedrock-runtime':'3.936.0','@aws-sdk/client-sts':'3.936.0',
 '@aws-sdk/client-sso':'3.936.0','@aws-sdk/client-cognito-identity':'3.936.0','@aws-sdk/nested-clients':'3.936.0',
 '@azure/identity':'4.10.1','acorn':'8.15.0','fuse.js':'7.0.0','chokidar':'4.0.3','google-auth-library':'10.6.2',
 '@grpc/grpc-js':'1.14.4','openid-client':'5.7.1','gaxios':'7.1.4','highlight.js':'11.11.1','react':'19.2.0',
}
def pick(name):
    if name in DECLARED: return name, DECLARED[name], 'declared'
    try:
        t = json.loads(subprocess.run(['npm','view',name,'time','--json'],capture_output=True,text=True,timeout=120).stdout)
    except Exception as e:
        return name, None, 'err'
    vs = [(d,v) for v,d in t.items() if v not in ('created','modified') and d < CUT and re.fullmatch(r'\d+\.\d+\.\d+', v)]
    if not vs: return name, None, 'none'
    def key(v): return tuple(int(x) for x in v.split('.'))
    best = max((v for d,v in vs), key=key)
    return name, best, 'newest-before-cut'
names = [l.strip() for l in open(sys.argv[1]) if l.strip() and not l.startswith('#')]
with ThreadPoolExecutor(16) as ex:
    res = list(ex.map(pick, names))
out = {n:{'version':v,'why':w} for n,v,w in res}
json.dump(out, open(sys.argv[2],'w'), indent=1)
for n,v,w in res: print(n,v,w)
