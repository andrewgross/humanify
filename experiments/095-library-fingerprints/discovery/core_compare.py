"""exp095 discovery: the PIPELINE's discovery (humanify_core::libdetect::discovery, via proto's exp095-core-discover)
against the lab prototype (discover.py) on CC 2.1.216, unit by unit, scored the same way.

Usage: python3 core_compare.py <edges.tsv> <declared.jsonl> <core-declared.json> <core-C.json> [out.json]

Core claims are spans; they map onto the lab's units by (start, end), else by containment (a core unit holding lab
units). The prototype side runs discover.py's own seeds, fixpoint, isolation veto, DOWN-CLASS and UP queue.
"""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from discover import (State, closure_sizes, cls, declared_seeds, down_class, est, fixpoint,  # noqa: E402
                      isolation_veto, round_down, round_glue, round_module, score, stepb_claims)
from graphlib import Graph, load_labels  # noqa: E402

RULES = [('down', round_down), ('module', round_module), ('glue', round_glue)]


class Mapper:
    def __init__(self, g):
        self.g = g
        self.by_span = {(r['s'], r['e']): i for i, r in g.rows.items()}
        self.sorted = sorted(g.rows, key=lambda i: g.rows[i]['s'])
        self.starts = [g.rows[i]['s'] for i in self.sorted]

    def units(self, s, e):
        """Lab units for a core span: the exact one, else every lab unit inside it."""
        if (s, e) in self.by_span:
            return [self.by_span[(s, e)]]
        import bisect
        k = bisect.bisect_left(self.starts, s)
        out = []
        while k < len(self.sorted) and self.starts[k] < e:
            u = self.sorted[k]
            if self.g.rows[u]['e'] <= e:
                out.append(u)
            k += 1
        return out


class CoreState:
    """discover.State's read surface (pkg, how) for core claims."""
    def __init__(self, g, mapper, report):
        self.g = g
        self.pkg, self.how, self.round_of = {}, {}, {}
        self.unmapped = 0
        for c in report['claims']:
            us = mapper.units(c['start'], c['end'])
            if not us:
                self.unmapped += 1
            for u in us:
                self.pkg[u] = c['package']
                self.how[u] = c['rule'] if c['rule'] != 'seed' else 'seed'
                self.round_of[u] = c['round']


def kb(g, units):
    return round(sum(g.bytes[u] for u in units if u in g.in_pop) / 1e3, 1)


def describe(g, units, sample_by_unit, n=12):
    rows = []
    for u in sorted(units, key=lambda u: -g.bytes[u])[:n]:
        r = g.rows[u]
        rows.append({'unit': u, 'bytes': g.bytes[u], 'kind': g.kind[u], 'grp': g.grp.get(u),
                     'label': sample_by_unit.get(u)})
    return rows


def compare(g, mapper, meta, sample, traps, proto_st, core_report, name):
    core = CoreState(g, mapper, core_report)
    sample_by_unit = {r['unit']: cls(r) + (':' + r['pkg'] if r.get('pkg') else '') for r in sample}
    p, c = set(proto_st.pkg), set(core.pkg)
    p_grown = {u for u in p if proto_st.how[u] != 'seed'}
    c_grown = {u for u in c if core.how[u] != 'seed'}
    same_pkg = sum(1 for u in p & c if proto_st.pkg[u] == core.pkg[u])
    out = {
        'unmappedCoreClaims': core.unmapped,
        'proto': {'units': len(p), 'srcKB': kb(g, p), 'grownKB': kb(g, p_grown), 'score': score(g, proto_st, meta, sample, traps)},
        'core': {'units': len(c), 'srcKB': kb(g, c), 'grownKB': kb(g, c_grown), 'score': score(g, core, meta, sample, traps)},
        'both': {'units': len(p & c), 'samePackage': same_pkg},
        'onlyProto': {'units': len(p - c), 'srcKB': kb(g, p - c),
                      'byRule': dict(collections.Counter(proto_st.how[u] for u in p - c)),
                      'largest': describe(g, p - c, sample_by_unit)},
        'onlyCore': {'units': len(c - p), 'srcKB': kb(g, c - p),
                     'byRule': dict(collections.Counter(core.how[u] for u in c - p)),
                     'largest': describe(g, c - p, sample_by_unit)},
        'grownOnlyProto': {'units': len(p_grown - c), 'srcKB': kb(g, p_grown - c)},
        'grownOnlyCore': {'units': len(c_grown - p), 'srcKB': kb(g, c_grown - p)},
        'samplePackageDiffers': [(u, proto_st.pkg[u], core.pkg[u]) for u in sorted(p & c)
                                 if proto_st.pkg[u] != core.pkg[u] and u in sample_by_unit],
    }
    print(name, json.dumps({k: v for k, v in out.items() if k not in ('onlyProto', 'onlyCore')}, default=str)[:3000],
          flush=True)
    return out, core


def region_compare(g, mapper, meta, sample, proto_units, core_rows):
    core = set()
    for r in core_rows:
        core.update(mapper.units(r['start'], r['end']))
    lib = lambda r: cls(r) == 'library'  # noqa: E731
    app = lambda r: cls(r) == 'app'  # noqa: E731
    def side(units):
        return {'units': len(units), 'srcKB': kb(g, units),
                'libEstKB': round(est(meta, sample, lambda r: lib(r) and r['unit'] in units) / 1e3),
                'appEstKB': round(est(meta, sample, lambda r: app(r) and r['unit'] in units) / 1e3),
                'sampleLabels': dict(collections.Counter(cls(r) for r in sample if r['unit'] in units))}
    return {'proto': side(proto_units), 'core': side(core), 'both': len(proto_units & core),
            'onlyProtoKB': kb(g, proto_units - core), 'onlyCoreKB': kb(g, core - proto_units)}


def main():
    edges, declared_path, core_dec_path, core_c_path = sys.argv[1:5]
    out_path = sys.argv[5] if len(sys.argv) > 5 else None
    g = Graph(edges)
    meta, sample, traps = load_labels()
    mapper = Mapper(g)
    dec = declared_seeds(g, declared_path)
    clean = lambda s: {u: p for u, p in s.items() if u in g.rows and u not in g.runtime and u not in g.cjs}  # noqa: E731
    report = {}

    # Seeds: the declared readers + banners, prototype vs core.
    core_dec = json.load(open(core_dec_path))
    core_seed_units = {}
    for c in core_dec['claims']:
        if c['rule'] == 'seed':
            for u in mapper.units(c['start'], c['end']):
                core_seed_units[u] = (c['package'], c['evidence']['kind'])
    pdec = clean(dec)
    report['declaredSeeds'] = {
        'proto': {'units': len(pdec), 'srcKB': kb(g, pdec)},
        'core': {'units': len(core_seed_units), 'srcKB': kb(g, core_seed_units)},
        'onlyProto': [(u, pdec[u], g.bytes[u]) for u in sorted(set(pdec) - set(core_seed_units))],
        'onlyCore': [(u, core_seed_units[u], g.bytes[u]) for u in sorted(set(core_seed_units) - set(pdec))],
        'packageDiffers': [(u, pdec[u], core_seed_units[u][0]) for u in sorted(set(pdec) & set(core_seed_units))
                           if pdec[u] != core_seed_units[u][0]],
    }
    print('seeds', json.dumps({k: (v if k in ('proto', 'core') else len(v)) for k, v in report['declaredSeeds'].items()}))

    # Declared seeds only.
    st_dec, rounds_dec = fixpoint(g, pdec, RULES)
    report['declared'], _ = compare(g, mapper, meta, sample, traps, st_dec, core_dec, 'declared')
    report['declared']['protoRounds'] = [round(r['srcBytes'] / 1e3, 1) for r in rounds_dec]

    # Declared + Step B (index C) + isolation veto.
    core_c = json.load(open(core_c_path))
    full = clean({**stepb_claims('C'), **dec})
    iso = isolation_veto(g, full, set(dec))
    st_c, rounds_c = fixpoint(g, iso, RULES)
    report['declaredC'], core_st = compare(g, mapper, meta, sample, traps, st_c, core_c, 'declared+C/isolation')
    report['declaredC']['protoRounds'] = [round(r['srcBytes'] / 1e3, 1) for r in rounds_c]
    report['declaredC']['coreRounds'] = [round(r['statementBytes'] / 1e3, 1) for r in core_c['rounds']]
    vet_p = set(full) - set(iso)
    vet_c = set()
    for r in core_c['vetoed']:
        vet_c.update(mapper.units(r['start'], r['end']))
    report['veto'] = {'proto': {'units': len(vet_p), 'srcKB': kb(g, vet_p)},
                      'core': {'units': len(vet_c), 'srcKB': kb(g, vet_c)},
                      'both': len(vet_p & vet_c), 'onlyProto': sorted(vet_p - vet_c), 'onlyCore': sorted(vet_c - vet_p),
                      'trapsVetoedCore': [t['trap'] for t in traps if t['unit'] in vet_c]}
    print('veto', json.dumps(report['veto'])[:1500])

    # DOWN-CLASS (floor 30) and the UP queue (256 KB), after the fixpoint.
    report['downClass'] = region_compare(g, mapper, meta, sample, down_class(g, st_c, 30), core_c['libraryClass'])
    print('downClass', json.dumps(report['downClass']))
    sizes = closure_sizes(g, st_c, 256_000)
    up = {v for v, sz in sizes.items() if sz is not None and sz <= 256_000 and v in g.in_pop
          and any(w in st_c.pkg for w in g.out[v])}
    report['queue'] = region_compare(g, mapper, meta, sample, up, core_c['queue'])
    print('queue', json.dumps(report['queue']))
    if out_path:
        json.dump(report, open(out_path, 'w'), indent=1, default=str)


if __name__ == '__main__':
    main()
