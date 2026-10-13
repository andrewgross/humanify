"""Step B: find library code in a bundle WITHOUT module boundaries, scored against sourcemap truth.

Index  = fingerprints (canonical function hashes + leaf-statement hashes, + long string literals) of library code
         from a chosen set of lab builds, each mapped to the set of packages it occurs in.
Query  = a bundle, read as an ordered list of leaf statements (top-level statements, and statements inside module
         wrappers). The detector never sees the labels.

Detector (each strategy adds to the previous):
  S1 exact   a statement is library P when >= COVER of its tokens are covered by index hits for P
             (maximal functions with >= NMIN tokens, or the whole statement's hash).
  S2 gapfill an unmatched statement between two statements labelled P, at most GAP statements / GAPB bytes away,
             becomes P (growth along emitted order).
  S3 group   a lazy-ESM / CJS wrapper group (the wrapper plus the hoisted statements before it, back to the previous
             wrapper) that is >= GROUPMIN covered by P becomes P entirely.
  S4 strings S3, plus long string literals (>= 12 chars) unique to one package count as evidence for a statement.

Scoring is per byte of leaf statements: truth library/app comes from the lab labels.
usage: python3 scripts/stepb.py <experiment>   (see EXPERIMENTS at the bottom)
"""
import json, os, sys, collections, glob, pickle, random
from multiprocessing import Pool

ROOT = '/work/exp095-library-fingerprints/bundler-variation-and-boundaries'
UNITS = ROOT + '/data/units'
NONLIB = ('<app>', '<runtime>', '<unmapped>', '<bundler-helper>')


def load_units(b):
    """ordered leaf statements of one build with truth + group id"""
    labs = json.load(open(f'{UNITS}/{b}.labels.json'))
    rows = [json.loads(l) for l in open(f'{UNITS}/{b}.jsonl')]
    out = []
    group = 0
    pending = []  # depth-0 non-wrapper units since the last wrapper
    wrapper_of = {}
    for r in rows:
        if r['d'] == 0 and r['w']:
            wrapper_of[r['i']] = group
            for u in pending:
                u['g'] = group
            pending = []
            group += 1
    # second pass to assign
    group = 0
    pend = []
    for r in rows:
        lab = labs.get(str(r['lab']))
        truth = (lab[0], lab[1], lab[3], lab[2]) if lab else ('<none>', None, None, None)  # pkg, version, kind, file/member
        if r['d'] == 0 and r['w']:
            for u in pend:
                u['g'] = group
            pend = []
            g = group
            group += 1
            continue
        u = {'skd': r.get('skd', 99), 's': r['s'], 'e': r['e'], 'n': r['n'], 'h3': r['h3'], 'h4': r['h4'], 'fns': r['fns'], 'ls': r['ls'], 'truth': truth, 'g': None}
        if r['d'] == 1:
            u['g'] = wrapper_of.get(r['p'])
        else:
            pend.append(u)
        out.append(u)
    for u in pend:
        u['g'] = None  # eager tail: no group
    return out


def index_from_builds(builds, level='h3'):
    """hash -> set(pkg); also hash->set(pkg@ver). Separate tables: fn, stmt, str. Non-library occurrences recorded as
    the pseudo-package '<nonlib>' so they can veto."""
    fi = collections.defaultdict(set)
    si = collections.defaultdict(set)
    li = collections.defaultdict(set)
    fn_n = {}
    with Pool(48) as p:
        for b, part in p.imap_unordered(_index_one, [(b, level) for b in builds], chunksize=8):
            fn_n.update(part[3])
            for k, v in part[0].items():
                fi[k] |= v
            for k, v in part[1].items():
                si[k] |= v
            for k, v in part[2].items():
                li[k] |= v
    return {'fn': dict(fi), 'stmt': dict(si), 'str': dict(li), 'level': level, 'fnN': fn_n}


def _index_one(arg):
    b, level = arg
    li_ = 5 if level == 'h3' else 6  # position inside fns rows: [s,e,n,h1,h2,h3,h4]
    fi = collections.defaultdict(set)
    si = collections.defaultdict(set)
    li = collections.defaultdict(set)
    fn_n = {}
    for u in load_units(b):
        pkg = u['truth'][0]
        tag = '<nonlib>' if pkg in NONLIB or pkg == '<none>' else pkg
        for f in u['fns']:
            fi[f[li_]].add(tag)
            fn_n[f[li_]] = f[2]
        si[u[level]].add(tag)
        for s in u['ls']:
            li[s].add(tag)
    return b, (dict(fi), dict(si), dict(li), fn_n)


def detect(units, idx, nmin=30, cover=0.5, gap=6, gapb=4000, groupmin=0.5, strategy='S3', exclude=None, maxpk=3, mincontent=0, support=0, strong=100, minfncontent=None):
    """returns list of predicted labels (pkg or None) per unit. exclude: a package to pretend is NOT in the index."""
    level = idx['level']
    li_ = 5 if level == 'h3' else 6

    def look(table, h):
        ps = table.get(h)
        if not ps or '<nonlib>' in ps:
            return None
        if exclude and exclude in ps:
            ps = ps - {exclude}
            if not ps:
                return None
        if len(ps) > maxpk:
            return None
        return ps

    pred = [None] * len(units)
    cov = [None] * len(units)
    # statements the index has seen as NON-library (bundler runtime helpers, app glue) are never grown over
    prev = None
    ev = [0] * len(units)
    veto = [bool(idx['stmt'].get(u[level]) and '<nonlib>' in idx['stmt'][u[level]] and u['n'] >= 8) for u in units]
    for i, u in enumerate(units):
        tot = max(u['n'], 1)
        score = collections.Counter()
        if u['n'] >= nmin and u['skd'] >= mincontent:
            ps = look(idx['stmt'], u[level])
            if ps:
                for p in ps:
                    score[p] = tot
        if not score:
            hits = []
            for f in u['fns']:
                if f[2] >= nmin and (len(f) < 8 or f[7] >= (mincontent if minfncontent is None else minfncontent)):
                    ps = look(idx['fn'], f[li_])
                    if ps:
                        hits.append((f[2], f[0], f[1], ps))
            hits.sort(key=lambda x: -x[0])
            taken = []
            for n, s, e, ps in hits:
                if any(not (e <= a or s >= b) for a, b in taken):
                    continue
                taken.append((s, e))
                for p in ps:
                    score[p] += n
        anchors = collections.Counter()
        if strategy[0] in 'AS' and strategy in ('S4', 'A1', 'A2', 'A3'):
            for s in u['ls']:
                ps = look(idx['str'], s)
                if ps and len(ps) == 1:
                    for p in ps:
                        anchors[p] += 1
        if strategy.startswith('A'):
            score = collections.Counter()  # strings only
            if len(anchors) == 1:
                pred[i] = next(iter(anchors))
            continue
        if score:
            best = max(score.values())
            cands = sorted(k for k, v in score.items() if v == best)
            # tie-break: the package of the previous labelled statement (locality), else alphabetical
            p = prev if prev in cands else cands[0]
            sc = best
            cov[i] = (p, sc / tot)
            if sc / tot >= cover or (strategy == 'S4' and sc / tot >= cover / 2 and anchors.get(p)):
                pred[i] = p
                ev[i] = sc
                prev = p
    if support:
        # seed-and-extend: a weak hit needs another hit of the same package within +-support statements
        keep = list(pred)
        for i, p in enumerate(pred):
            if p and ev[i] < strong:
                lo, hi = max(0, i - support), min(len(pred), i + support + 1)
                if not any(pred[j] == p for j in range(lo, hi) if j != i):
                    keep[i] = None
        pred = keep
    stage = 1 if strategy in ('S1', 'A1') else 2 if strategy in ('S2', 'A2') else 3
    return grow(units, pred, veto, stage, gap, gapb, groupmin)


def grow(units, pred, veto, stage, gap=6, gapb=4000, groupmin=0.5):
    """stage 1: none; 2: gap fill along emitted order; 3: + wrapper-group completion"""
    if stage == 1:
        return pred
    # S2 gap fill along emitted order
    last = {}  # pkg -> index of last labelled unit
    filled = list(pred)
    for i, p in enumerate(pred):
        if p is None:
            continue
        j = last.get(p)
        if j is not None and 1 < i - j <= gap + 1:
            span_bytes = units[i]['s'] - units[j]['e']
            gap_units = range(j + 1, i)
            if span_bytes <= gapb and all(pred[k] is None and not veto[k] for k in gap_units):
                for k in gap_units:
                    filled[k] = p
        last[p] = i
    pred = filled
    if stage == 2:
        return pred
    # S3 wrapper groups
    groups = collections.defaultdict(list)
    for i, u in enumerate(units):
        if u['g'] is not None:
            groups[u['g']].append(i)
    for g, ids in groups.items():
        tot = sum(max(units[i]['e'] - units[i]['s'], 1) for i in ids)
        c = collections.Counter()
        for i in ids:
            if pred[i]:
                c[pred[i]] += units[i]['e'] - units[i]['s']
        if c:
            p, b = c.most_common(1)[0]
            if b / tot >= groupmin:
                for i in ids:
                    if pred[i] is None and not veto[i]:
                        pred[i] = p
    return pred


def score(units, pred, pkg_ok=lambda t, p: t == p):
    """byte-level confusion. truth classes: lib / app (app+runtime+unmapped are 'not library')."""
    r = collections.Counter()
    for u, p in zip(units, pred):
        b = u['e'] - u['s']
        t = u['truth'][0]
        kind = u['truth'][2] or 'none'
        islib = not (t in NONLIB or t == '<none>')
        if islib:
            r['libBytes'] += b
            r[f'libBytes:{kind}'] += b
            if p:
                r['libFound'] += b
                r[f'libFound:{kind}'] += b
                if pkg_ok(t, p):
                    r['pkgRight'] += b
                else:
                    r['pkgWrong'] += b
        else:
            r[f'nonlibBytes:{t}'] += b
            if p:
                r[f'nonlibFlagged:{t}'] += b
            if t == '<app>':
                r['nonlibBytes'] += b
                if p:
                    r['nonlibFlagged'] += b
    return r


def summarize(r):
    pct = lambda a, b: round(100 * r[a] / r[b], 2) if r[b] else None
    out = {
        'libMB': round(r['libBytes'] / 1e6, 2),
        'recall%': pct('libFound', 'libBytes'),
        'pkgRight%ofFound': pct('pkgRight', 'libFound'),
        'appKB': round(r['nonlibBytes'] / 1e3, 1),
        'appFlagged%': pct('nonlibFlagged', 'nonlibBytes'),
        'appFlaggedBytes': r['nonlibFlagged'],
    }
    for k in ('cjs', 'lazy-esm', 'eager-esm'):
        out[f'recall%:{k}'] = pct(f'libFound:{k}', f'libBytes:{k}')
    for k in ('<app>', '<runtime>', '<unmapped>'):
        if r[f'nonlibBytes:{k}']:
            out[f'flagged:{k}'] = f"{r[f'nonlibFlagged:{k}']}/{r[f'nonlibBytes:{k}']}"
    return out
