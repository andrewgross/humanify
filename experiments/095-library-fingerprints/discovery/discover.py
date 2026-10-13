"""exp095 library discovery — graph-closure candidates + leaf-inward fixpoint on CC 2.1.216 (lab, not pipeline).

Usage: python3 discover.py <edges.tsv> <declared.jsonl> [out.json]

Externals X (never candidates): the bundler runtime (group 0), CJS factories (already vendor by bundler semantics),
and every IDENTIFIED library node. D_X(v) = v's downward closure over unidentified nodes (stops at X) — the minimal
closed region containing v (every out-edge of D_X(v) lands in D_X(v) or X, so the region's edges with the rest point
one way by construction).

Measured:
  1. candidates: v with |D_X(v)| <= S bytes, for several S, with X = runtime+CJS (no seeds) and X after the fixpoint;
  2. the fixpoint: seeds (declared readers and/or Step B claims) become X; per round, two naming rules
       DOWN   — an unidentified node every one of whose callers (in-edges) is library code of ONE package P, and whose
                closure stays inside X ∪ itself: it is P's private code -> P.
       MODULE — an unidentified unit in a Bun lazy module (group) that already holds claims of exactly one package P,
                whose closure D_X(u) stays inside the module (or X), and none of whose callers outside the module are
                unidentified app code -> P. (The bounded, closure-guarded version of Step B's neighbour fill.)
     iterate until nothing changes; report rounds and bytes per round;
  3. every claim scored against negatives/labels.jsonl (stratified estimator) and traps.jsonl.
"""
import collections
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from graphlib import BUNDLE, Graph, cls, est, load_labels, scc, stepb_claims  # noqa: E402


def fam(p):
    if p is None:
        return None
    if p.startswith(('@aws-sdk/', '@smithy/', '@aws-crypto/')):
        return 'aws'
    if p.startswith(('@azure/', '@typespec/')):
        return 'azure'
    if p.startswith('@anthropic-ai/'):
        return 'anthropic'
    if p.startswith('@bufbuild/'):
        return 'bufbuild'
    if p.startswith('@modelcontextprotocol/'):
        return 'mcp'
    return p


def unit_at(g, starts, pos):
    """Deepest population unit containing pos, else the next unit after pos (a banner precedes its statement)."""
    import bisect
    k = bisect.bisect_right(starts, pos) - 1
    best = None
    for j in range(max(k - 3, 0), min(k + 2, len(starts))):
        u = g.sorted_units[j]
        r = g.rows[u]
        if r['s'] <= pos < r['e'] and (best is None or g.rows[best]['s'] < r['s']):
            best = u
    if best is None and k + 1 < len(starts):
        best = g.sorted_units[k + 1]
    return best


def declared_seeds(g, declared_path):
    g.sorted_units = sorted(g.in_pop, key=lambda u: g.rows[u]['s'])
    starts = [g.rows[u]['s'] for u in g.sorted_units]
    seeds = {}
    for line in open(declared_path):
        d = json.loads(line)
        pkg = d.get('package')
        if d['reader'] == 'aws-sdk-client':
            pkg = '@aws-sdk/client-' + re.sub(r'(?<!^)(?=[A-Z])', '-', (d.get('client') or 'x').replace('Client', '')).lower()
        if not pkg:
            continue  # version constants name no package
        u = unit_at(g, starts, d['end'] - 1)
        if u is not None:
            seeds[u] = pkg
    text = open(BUNDLE).read()
    for m in re.finditer(r'/\*!\s*(@?[a-z0-9][\w./-]*?)(?: v\d[\w.-]*| - | -|\.)', text):
        name = m.group(1)
        if name in ('',):
            continue
        u = unit_at(g, starts, m.end())
        if u is not None and u in g.in_pop:
            seeds[u] = name
    return seeds


class State:
    def __init__(self, g, seeds):
        self.g = g
        self.base = g.runtime | g.cjs
        self.pkg = dict(seeds)  # node -> package (identified library code)
        self.how = {u: 'seed' for u in seeds}
        self.round_of = {u: 0 for u in seeds}

    def ext(self, v):
        return v in self.base or v in self.pkg

    def closure(self, v, cap=None, stop=None):
        """D_X(v) as a set (None when it exceeds `cap` bytes or leaves `stop`)."""
        seen = {v}
        st = [v]
        b = 0
        while st:
            x = st.pop()
            b += self.g.bytes[x]
            if cap is not None and b > cap:
                return None
            for w in self.g.out[x]:
                if w in seen or self.ext(w):
                    continue
                if stop is not None and not stop(w):
                    return None
                seen.add(w)
                st.append(w)
        return seen


def closure_sizes(g, st, cap):
    """|D_X(v)| for every unidentified node, capped (None = over cap), via SCC condensation."""
    V = [v for v in g.rows if not st.ext(v)]
    succ = lambda v: [w for w in g.out[v] if not st.ext(w)]  # noqa: E731
    comps = scc(V, succ)
    cid = {}
    for k, c in enumerate(comps):
        for v in c:
            cid[v] = k
    cb = [sum(g.bytes[v] for v in c) for c in comps]
    clo = [None] * len(comps)
    for k, c in enumerate(comps):
        s = {k}
        big = False
        for v in c:
            for w in succ(v):
                j = cid[w]
                if j == k:
                    continue
                if clo[j] is None:
                    big = True
                    break
                s |= clo[j]
            if big:
                break
        if not big and sum(cb[j] for j in s) > cap:
            big = True
        clo[k] = None if big else frozenset(s)
    out = {}
    for v in V:
        c = clo[cid[v]]
        out[v] = None if c is None else sum(cb[j] for j in c)
    return out


def uses_library(g, st, v, members):
    """Does the closed region `members` reference identified library code (not just runtime/globals)?"""
    return any(w in st.pkg for x in members for w in g.out[x])


def round_down(g, st):
    """DOWN: unidentified node whose callers are ALL identified as one package P and whose closure stays in X."""
    new = {}
    for v in g.rows:
        if st.ext(v) or v not in g.in_pop:
            continue
        callers = g.inn[v]
        if not callers:
            continue
        pk = {st.pkg.get(u) for u in callers}
        if len(pk) != 1 or None in pk:
            continue
        P = pk.pop()
        # wrapper -> body edges make a lazy module's init a caller of each body statement: an init only
        # counts once the init itself is P (rule GLUE).
        new[v] = P
    return new


def round_glue(g, st):
    """GLUE: a lazy-module init wrapper whose body statements are all ONE package P is P's bundler glue."""
    new = {}
    for w, cs in g.children.items():
        if st.ext(w) or g.kind.get(w) != 'esm':
            continue
        pk = {st.pkg.get(c) for c in cs}
        if len(pk) == 1 and None not in pk:
            new[w] = pk.pop()
    return new


def round_module(g, st, guard=True):
    """MODULE: fill unidentified units of a Bun lazy module whose identified units are all ONE package."""
    by_grp = collections.defaultdict(list)
    for u in g.in_pop:
        if g.grp.get(u) not in (None, 0):
            by_grp[g.grp[u]].append(u)
    new = {}
    for G, us in by_grp.items():
        pk = {st.pkg[u] for u in us if u in st.pkg}
        if len(pk) != 1:
            continue
        P = next(iter(pk))
        ids = set(us)
        for u in us:
            if st.ext(u):
                continue
            if guard:
                # closed inside the module (or X) ...
                if st.closure(u, stop=lambda w: w in ids) is None:
                    continue
                # ... and not called by unidentified code outside the module (app -> region is allowed only
                # through identified P entry points: a unit app code calls directly needs its own evidence).
                if any((c not in ids) and not st.ext(c) for c in g.inn[u]):
                    continue
            new[u] = P
    return new


def down_class(g, st, floor):
    """DOWN-CLASS: the downward closure of identified library code over unidentified nodes, started only from claims
    of >= `floor` bytes (a content-free claim such as a bare `var X={}` namespace can own a whole app module through
    its population statement). Library CLASS only — the package is unknown (a dependency of the caller)."""
    seen = set()
    stack = [w for u in st.pkg if g.bytes[u] >= floor for w in g.out[u]]
    while stack:
        x = stack.pop()
        if x in seen or st.ext(x):
            continue
        seen.add(x)
        stack.extend(g.out[x])
    return seen


def code_regions(g, st, sizes, cap):
    """Candidates whose closed region contains at least one function (drops pure data: doc text, tables)."""
    fns = {i: bool(r.get('fns')) for i, r in g.rows.items()}
    out = set()
    for u in g.in_pop:
        if sizes.get(u) is None or sizes[u] > cap:
            continue
        c = st.closure(u, cap=cap)
        if c is not None and any(fns[x] for x in c):
            out.add(u)
    return out


def isolated(g, claims):
    """ISOLATION guard: a claim with no graph neighbour (caller or callee, runtime excluded) claimed as the SAME
    package. A fork keeps a library's leaf pieces (tables, helpers) while the app rewrites everything around them,
    so the kept piece's only callers are app code."""
    out = set()
    for u, p in claims.items():
        nb = (g.out[u] | g.inn[u]) - g.runtime
        if not any(fam(claims.get(w)) == fam(p) for w in nb):
            out.add(u)
    return out


def isolation_veto(g, seeds, exempt):
    """Drop INDEX seeds that are isolated in the post-fixpoint claim set (declared seeds are exempt: a declaration
    is its own evidence). Repeats until stable, since a dropped seed can strand what it supported."""
    seeds = {u: p for u, p in seeds.items() if u in g.rows and u not in g.runtime and u not in g.cjs}
    while True:
        st, _ = fixpoint(g, seeds, [('down', round_down), ('module', round_module), ('glue', round_glue)])
        drop = {u for u in isolated(g, st.pkg) if u in seeds and u not in exempt}
        if not drop:
            return seeds
        seeds = {u: p for u, p in seeds.items() if u not in drop}


def fixpoint(g, seeds, rules, max_rounds=50):
    st = State(g, seeds)
    rounds = []
    for k in range(1, max_rounds + 1):
        added = {}
        for name, fn in rules:
            for v, P in fn(g, st).items():
                if v not in added and not st.ext(v):
                    added[v] = (P, name)
        if not added:
            break
        for v, (P, name) in added.items():
            st.pkg[v] = P
            st.how[v] = name
            st.round_of[v] = k
        rounds.append({'round': k, 'nodes': len(added), 'bytes': sum(g.bytes[v] for v in added),
                       'srcBytes': sum(g.bytes[v] for v in added if v in g.in_pop),
                       'byRule': dict(collections.Counter(n for _, n in added.values()))})
    return st, rounds


def score(g, st, meta, sample, traps, only=None):
    """Byte-weighted scores of the identified set (optionally only nodes passing `only`)."""
    def claimed(r):
        u = r['unit']
        return u in st.pkg and (only is None or only(u))
    lib = lambda r: cls(r) == 'library'  # noqa: E731
    app = lambda r: cls(r) == 'app'  # noqa: E731
    lib_tot = est(meta, sample, lib)
    out = {
        'libRecall': round(est(meta, sample, lambda r: lib(r) and claimed(r)) / lib_tot, 4),
        'libRightFamily': round(est(meta, sample, lambda r: lib(r) and claimed(r)
                                    and fam(st.pkg[r['unit']]) == fam(r['pkg'])) / lib_tot, 4),
        'libRightPackage': round(est(meta, sample, lambda r: lib(r) and claimed(r)
                                     and st.pkg[r['unit']] == r['pkg']) / lib_tot, 4),
        'libWrongSampleUnits': sorted((r['unit'], r['pkg'], st.pkg[r['unit']]) for r in sample
                                      if lib(r) and claimed(r) and st.pkg[r['unit']] != r['pkg']),
        'appClaimedEstKB': round(est(meta, sample, lambda r: app(r) and claimed(r)) / 1e3, 1),
        'appClaimedSampleUnits': sorted(r['unit'] for r in sample if app(r) and claimed(r)),
        'helperClaimedSampleUnits': sorted(r['unit'] for r in sample if cls(r) == 'compiler-helper' and claimed(r)),
        'trapsClaimed': [(t['trap'], t['unit'], st.pkg[t['unit']], st.how[t['unit']]) for t in traps
                         if t['unit'] in st.pkg and (only is None or only(t['unit'])) and cls(t) != 'library'],
        'srcBytesClaimed': sum(g.bytes[u] for u in st.pkg if u in g.in_pop and (only is None or only(u))),
    }
    return out


def candidates(g, st, meta, sample, traps, caps):
    """Closed-region candidates: unidentified v with |D_X(v)| <= S."""
    sizes = closure_sizes(g, st, max(caps))
    lib = lambda r: cls(r) == 'library'  # noqa: E731
    app = lambda r: cls(r) == 'app'  # noqa: E731
    fork = lambda r: bool(r.get('fork_of'))  # noqa: E731
    lib_tot = est(meta, sample, lib)
    res = {}
    for S in caps:
        cand = lambda u: u in sizes and sizes[u] is not None and sizes[u] <= S  # noqa: E731
        libc = est(meta, sample, lambda r: lib(r) and cand(r['unit']))
        appc = est(meta, sample, lambda r: app(r) and cand(r['unit']))
        res[S] = {
            'libKB': round(libc / 1e3), 'libRecallOfRemaining': None,
            'libRecallOfAll': round(libc / lib_tot, 3),
            'appKB': round(appc / 1e3),
            'forkKB': round(est(meta, sample, lambda r: fork(r) and cand(r['unit'])) / 1e3),
            'libShareOfCandidates': round(libc / (libc + appc), 3) if libc + appc else None,
            'srcCandidateKB': round(sum(g.bytes[u] for u in g.in_pop if cand(u)) / 1e3),
            'traps': sum(1 for t in traps if cand(t['unit']) and cls(t) != 'library'),
        }
        rem = est(meta, sample, lambda r: lib(r) and not st.ext(r['unit']))
        res[S]['libRecallOfRemaining'] = round(libc / rem, 3) if rem else None
    return res, sizes


def main():
    edges, declared_path = sys.argv[1], sys.argv[2]
    out_path = sys.argv[3] if len(sys.argv) > 3 else None
    g = Graph(edges)
    meta, sample, traps = load_labels()
    dec = declared_seeds(g, declared_path)
    seedsets = {
        'none': {},
        'declared': dec,
        'stepB-A': stepb_claims('A'),
        'stepB-C': stepb_claims('C'),
        'declared+stepB-C': {**stepb_claims('C'), **dec},
    }
    seedsets['declared+stepB-C/isolation'] = isolation_veto(g, seedsets['declared+stepB-C'], set(dec))
    caps = [4_000, 64_000, 256_000, 1_000_000]
    report = {'graph': {'nodes': len(g.rows), 'edges': sum(len(v) for v in g.out.values()),
                        'srcUnits': len(g.in_pop), 'srcBytes': sum(g.bytes[u] for u in g.in_pop)},
              'libTotalEstKB': round(est(meta, sample, lambda r: cls(r) == 'library') / 1e3),
              'appTotalEstKB': round(est(meta, sample, lambda r: cls(r) == 'app') / 1e3),
              'runs': {}}
    rulesets = {
        'down': [('down', round_down), ('glue', round_glue)],
        'down+module': [('down', round_down), ('module', round_module), ('glue', round_glue)],
        'module-unguarded': [('module-unguarded', lambda g, st: round_module(g, st, guard=False)),
                             ('glue', round_glue)],
    }
    for sname, seeds in seedsets.items():
        seeds = {u: p for u, p in seeds.items() if u in g.rows and u not in g.runtime and u not in g.cjs}
        st0 = State(g, seeds)
        run = {'seedUnits': len(seeds), 'seedBytes': sum(g.bytes[u] for u in seeds),
               'seedScore': score(g, st0, meta, sample, traps)}
        cand0, _ = candidates(g, st0, meta, sample, traps, caps)
        run['candidatesBefore'] = cand0
        if seeds:
            for rname, rules in rulesets.items():
                st, rounds = fixpoint(g, seeds, rules)
                grown = lambda u, st=st: st.how.get(u) != 'seed'  # noqa: E731
                run[rname] = {'rounds': rounds, 'grownSrcBytes': sum(g.bytes[u] for u in st.pkg
                                                                     if u in g.in_pop and st.how[u] != 'seed'), 'after': score(g, st, meta, sample, traps),
                              'grownOnly': score(g, st, meta, sample, traps, only=grown)}
                if rname == 'down+module':
                    run['candidatesAfter'], _ = candidates(g, st, meta, sample, traps, caps)
                    # closed regions that use an identified library (Andrew's "regions calling only it")
                    sizes = closure_sizes(g, st, 256_000)
                    up = set()
                    for v, sz in sizes.items():
                        if sz is not None and sz <= 256_000 and v in g.in_pop:
                            if any(w in st.pkg for w in g.out[v]):
                                up.add(v)
                    lib = lambda r: cls(r) == 'library'  # noqa: E731
                    app = lambda r: cls(r) == 'app'  # noqa: E731
                    run['upCandidates'] = {
                        'libKB': round(est(meta, sample, lambda r: lib(r) and r['unit'] in up) / 1e3),
                        'appKB': round(est(meta, sample, lambda r: app(r) and r['unit'] in up) / 1e3),
                        'srcKB': round(sum(g.bytes[u] for u in up) / 1e3),
                        'traps': [t['trap'] for t in traps if t['unit'] in up and cls(t) != 'library'],
                    }
        if sname == 'none':
            sizes = closure_sizes(g, st0, 256_000)
            run['codeCandidates'] = {}
            for S in (4_000, 64_000, 256_000):
                C = code_regions(g, st0, sizes, S)
                lk = est(meta, sample, lambda r: cls(r) == 'library' and r['unit'] in C)
                ak = est(meta, sample, lambda r: cls(r) == 'app' and r['unit'] in C)
                run['codeCandidates'][S] = {'libKB': round(lk / 1e3), 'appKB': round(ak / 1e3),
                                            'libShare': round(lk / (lk + ak), 3),
                                            'srcKB': round(sum(g.bytes[u] for u in C) / 1e3)}
        if sname == 'declared+stepB-C/isolation':
            st, _ = fixpoint(g, seeds, rulesets['down+module'])
            run['downClass'] = {}
            for floor in (0, 30):
                C = down_class(g, st, floor)
                run['downClass'][floor] = {
                    'srcBytes': sum(g.bytes[w] for w in C if w in g.in_pop),
                    'sampleLabels': dict(collections.Counter(cls(r) for r in sample if r['unit'] in C)),
                    'libEstKB': round(est(meta, sample, lambda r: cls(r) == 'library' and r['unit'] in C) / 1e3),
                    'appEstKB': round(est(meta, sample, lambda r: cls(r) == 'app' and r['unit'] in C) / 1e3),
                    'traps': [t['trap'] for t in traps if t['unit'] in C and cls(t) != 'library'],
                }
            full = {u for u in seedsets['declared+stepB-C'] if u in g.rows and u not in g.runtime and u not in g.cjs}
            run['vetoDropped'] = {'units': len(full - set(seeds)), 'bytes': sum(g.bytes[u] for u in full - set(seeds))}
        report['runs'][sname] = run
        print(sname, json.dumps(run, default=str)[:3000], flush=True)
    if out_path:
        json.dump(report, open(out_path, 'w'), indent=1, default=str)


if __name__ == '__main__':
    main()
