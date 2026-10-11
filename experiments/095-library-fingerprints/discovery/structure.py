"""exp095 discovery: SCC structure of the reference graph (giant app core, what reaches it) vs the labels.
Usage: python3 structure.py <edges.tsv>"""
import sys, collections
sys.path.insert(0, __import__('os').path.dirname(__import__('os').path.abspath(__file__)))
from graphlib import *
g = Graph(sys.argv[1])
ext = g.runtime | g.cjs
V = [v for v in g.rows if v not in ext]
succ = lambda v: [w for w in g.out[v] if w not in ext]
comps = scc(V, succ)
comps.sort(key=lambda c: -sum(g.bytes[v] for v in c))
tot = sum(g.bytes[v] for v in V)
print('noncjs nodes', len(V), 'bytes', tot, 'sccs', len(comps))
for c in comps[:8]:
    print(' scc nodes', len(c), 'bytes', sum(g.bytes[v] for v in c))
giant = set(comps[0])
anc = set(giant); st = list(giant)
while st:
    v = st.pop()
    for u in g.inn[v]:
        if u not in anc and u not in ext:
            anc.add(u); st.append(u)
print('reach giant: nodes', len(anc), 'bytes', sum(g.bytes[v] for v in anc))
meta, sample, traps = load_labels()
for name, f in [('lib', lambda r: cls(r)=='library'), ('app', lambda r: cls(r)=='app')]:
    for where, S in [('giant', giant), ('reach-giant(not giant)', anc-giant), ('below', set(V)-anc)]:
        print(name, where, round(est(meta, sample, lambda r: f(r) and r['unit'] in S)/1e3), 'KB',
              sum(1 for r in sample if f(r) and r['unit'] in S))
print('traps below:', [(t['trap'],t['unit']) for t in traps if t['unit'] not in anc])
print('traps in giant/reach:', collections.Counter(t['trap'] for t in traps if t['unit'] in anc))
