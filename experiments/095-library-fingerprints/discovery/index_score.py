"""exp095 discovery: score core reports made WITH an in-run library index (exp095-core-discover INDEX=...) against
the negatives lane's labels on CC 2.1.216, the same way core_compare.py scores the lab's Step B seeds.

usage: python3 index_score.py <edges.tsv> <report.json> [<report.json> ...]
Per report: index seeds (by kind), their recall alone, then after the fixpoint; app units claimed (sample, est. KB);
traps claimed; library class and queue."""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from core_compare import CoreState, Mapper, region_compare  # noqa: E402
from discover import score  # noqa: E402
from graphlib import Graph, cls, load_labels  # noqa: E402


class Only:
    """A CoreState restricted to some claims."""
    def __init__(self, st, keep):
        self.pkg = {u: p for u, p in st.pkg.items() if keep(u)}
        self.how = {u: st.how[u] for u in self.pkg}


def main():
    g = Graph(sys.argv[1])
    meta, sample, traps = load_labels()
    m = Mapper(g)
    for path in sys.argv[2:]:
        r = json.load(open(path))
        st = CoreState(g, m, r)
        kind = {}
        for c in r['claims']:
            for u in m.units(c['start'], c['end']):
                kind[u] = (c['evidence'] or {}).get('kind') if c['rule'] == 'seed' else c['rule']
        index_seeds = Only(st, lambda u: kind.get(u) in ('index', 'queueIndex'))
        before = Only(st, lambda u: kind.get(u) in ('index', 'queueIndex'))
        for v in r['vetoed']:
            for u in m.units(v['start'], v['end']):
                before.pkg[u] = v['package']
                before.how[u] = 'seed'
        seeds = Only(st, lambda u: st.how[u] == 'seed')
        def byte_recall(state):
            lib = [x for x in sample if cls(x) == 'library']
            return round(sum(x['bytes'] for x in lib if x['unit'] in state.pkg) / sum(x['bytes'] for x in lib), 4)
        out = {
            'sampleByteRecall': {'indexSeedsBeforeVeto': byte_recall(before), 'afterFixpoint': byte_recall(st)},
            'seedsByKind': r['seeds'],
            'indexSeedsBeforeVeto': score(g, before, meta, sample, traps),
            'indexSeedsAlone': score(g, index_seeds, meta, sample, traps),
            'allSeeds': score(g, seeds, meta, sample, traps),
            'afterFixpoint': score(g, st, meta, sample, traps),
            'grownKB': round(r['grownStatementBytes'] / 1e3, 1),
            'rounds': len(r['rounds']),
            'vetoed': (len(r['vetoed']), round(r['vetoedStatementBytes'] / 1e3, 1)),
            'libraryClass': region_compare(g, m, meta, sample, set(), r['libraryClass'])['core'],
            'queue': region_compare(g, m, meta, sample, set(), r['queue'])['core'],
        }
        print('==', path)
        for k, v in out.items():
            print(f'  {k}: {json.dumps(v, default=str)[:700]}')


if __name__ == '__main__':
    main()
