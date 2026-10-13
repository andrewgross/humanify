"""Shared loading + graph helpers for the exp095 discovery lab (CC 2.1.216 raw bundle).

Nodes = the exp095 unit model (raw.jsonl): depth-0 statements, lazy-ESM wrappers, CJS factories (one node each),
and the depth-1 statements of lazy-ESM init bodies. Edges = refgraph's TSV (resolved references between units)
plus wrapper -> body statement (calling the init runs them).
"""
import collections
import json
import os
import sys

LAB = '/work/exp095-library-fingerprints'
RAW = f'{LAB}/bundler-variation-and-boundaries/data/units/cc/min/cc-2.1.216/raw.jsonl'
NEG = f'{LAB}/negatives'
BUNDLE = '/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js'
sys.setrecursionlimit(10000)


class Graph:
    def __init__(self, edges_tsv):
        rows = [json.loads(l) for l in open(RAW)]
        wk = {r['i']: r['wk'] for r in rows if r['d'] == 0 and r['w']}
        self.rows = {}
        for r in rows:
            if r['d'] == 1 and wk.get(r['p']) == 'cjs':
                continue
            self.rows[r['i']] = r
        self.kind = {}
        for i, r in self.rows.items():
            if r['d'] == 0 and r['w']:
                self.kind[i] = r['wk']  # 'esm' wrapper or 'cjs' factory
            else:
                self.kind[i] = 'stmt'
        # bytes a node OWNS: wrappers own only their overhead (children are nodes)
        child_bytes = collections.Counter()
        self.children = collections.defaultdict(list)
        for i, r in self.rows.items():
            if r['d'] == 1:
                child_bytes[r['p']] += r['e'] - r['s']
                self.children[r['p']].append(i)
        self.bytes = {i: (r['e'] - r['s']) - child_bytes[i] for i, r in self.rows.items()}
        pop = [json.loads(l) for l in open(f'{NEG}/data/population.jsonl')]
        self.grp = {u['i']: u['grp'] for u in pop}
        self.in_pop = set(self.grp)
        self.out = collections.defaultdict(set)
        self.inn = collections.defaultdict(set)
        self.ekind = {}
        for line in open(edges_tsv):
            s, d, k = line.split('\t')
            s, d = int(s), int(d)
            if s in self.rows and d in self.rows:
                self.add(s, d, k.strip())
        for w, cs in self.children.items():
            for c in cs:
                self.add(w, c, 'body')
        self.runtime = {i for i, g in self.grp.items() if g == 0}
        self.cjs = {i for i, k in self.kind.items() if k == 'cjs'}

    def add(self, s, d, k):
        if s == d:
            return
        self.out[s].add(d)
        self.inn[d].add(s)
        self.ekind.setdefault((s, d), k)


def scc(nodes, succ):
    """Iterative Tarjan over `nodes` with successor fn; returns list of components (reverse topological)."""
    index = {}
    low = {}
    on = set()
    stack = []
    comps = []
    counter = 0
    for root in nodes:
        if root in index:
            continue
        work = [(root, iter(succ(root)))]
        index[root] = low[root] = counter
        counter += 1
        stack.append(root)
        on.add(root)
        while work:
            v, it = work[-1]
            advanced = False
            for w in it:
                if w not in index:
                    index[w] = low[w] = counter
                    counter += 1
                    stack.append(w)
                    on.add(w)
                    work.append((w, iter(succ(w))))
                    advanced = True
                    break
                if w in on:
                    low[v] = min(low[v], index[w])
            if advanced:
                continue
            work.pop()
            if work:
                u = work[-1][0]
                low[u] = min(low[u], low[v])
            if low[v] == index[v]:
                comp = []
                while True:
                    w = stack.pop()
                    on.discard(w)
                    comp.append(w)
                    if w == v:
                        break
                comps.append(comp)
    return comps


def load_labels():
    meta = json.load(open(f'{NEG}/data/sample-meta.json'))
    rows = [json.loads(l) for l in open(f'{NEG}/labels.jsonl')]
    sample = [r for r in rows if r['stratum'] in meta['strata']]
    traps = [json.loads(l) for l in open(f'{NEG}/traps.jsonl')]
    return meta, sample, traps


def cls(r):
    l = r['label']
    return 'library' if l.startswith('library:') else l


def est(meta, sample, f):
    """Stratified ratio estimator (negatives/scripts/estimate.py): estimated src bytes with property f."""
    by = collections.defaultdict(list)
    for r in sample:
        by[r['stratum']].append(r)
    tot = 0.0
    for h, m in meta['strata'].items():
        S = by[h]
        sb = sum(r['bytes'] for r in S)
        if sb:
            tot += m['bytes'] * sum(r['bytes'] for r in S if f(r)) / sb
    return tot


def stepb_claims(index='C'):
    d = json.load(open(f'{NEG}/data/stepb-pred-{index}.json'))
    return {int(k): v for k, v in d.items() if v}


if __name__ == '__main__':
    g = Graph(os.environ.get('EDGES', sys.argv[1]))
    print('nodes', len(g.rows), 'edges', sum(len(v) for v in g.out.values()))
