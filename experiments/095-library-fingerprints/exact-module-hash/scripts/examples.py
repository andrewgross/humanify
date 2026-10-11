"""Print small differing examples for a config pair: python3 examples.py <cfgA> <cfgB> <kindA> [variant] [n] [maxbytes]
   cfgB may be 'mode:<m1>:<m2>:<cfg>' to compare entry modes within one config."""
import sys, random, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/exact-module-hash/scripts')
from load import load, is_lib, LAB

a, b, kind = sys.argv[1], sys.argv[2], sys.argv[3]
v = sys.argv[4] if len(sys.argv) > 4 else 'coreM'
n = int(sys.argv[5]) if len(sys.argv) > 5 else 3
maxb = int(sys.argv[6]) if len(sys.argv) > 6 else 400
rows = [r for r in load() if is_lib(r) and r['ok'] and r['parts'] == 1]
if b.startswith('mode:'):
    _, m1, m2, cfg = b.split(':', 3)
    A = {(r['app'], r['pkg'], r['ver'], r['file']): r for r in rows if r['cfg'] == cfg and r['mode'] == m1}
    B = {(r['app'], r['pkg'], r['ver'], r['file']): r for r in rows if r['cfg'] == cfg and r['mode'] == m2}
else:
    A = {(r['app'], r['mode'], r['pkg'], r['ver'], r['file']): r for r in rows if r['cfg'] == a}
    B = {(r['app'], r['mode'], r['pkg'], r['ver'], r['file']): r for r in rows if r['cfg'] == b}
diff = [k for k in A if k in B and A[k]['kind'] == kind and A[k][v] != B[k][v] and A[k]['bytes'] < maxb]
random.seed(int(sys.argv[7]) if len(sys.argv) > 7 else 1)
print(len(diff), 'differing under', v)
for k in random.sample(diff, min(n, len(diff))):
    print('=' * 100, '\n', k)
    for r in (A[k], B[k]):
        print('---', r['cfg'], r['mode'], r['kind'], r['bytes'], 'B')
        print(open(f"{LAB}/{r['textPath']}").read()[:maxb + 200])
