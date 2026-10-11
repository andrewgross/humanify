"""Evidence cards for labelling: statement text + npm-source token evidence for the statement and its module group.
usage: python3 scripts/cards.py <sample.jsonl> <out.txt> [ids...]"""
import json, sys, collections, bisect
B = '/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js'
src = open(B, encoding='utf8').read()
idx = {}
for l in open('data/srcindex.tsv', encoding='utf8', newline='\n'):
    if '\t' not in l: continue
    t, ps = l.rstrip('\n').split('\t')
    idx[t] = sorted({p.rsplit('@', 1)[0] for p in ps.split('|')})
btok = json.load(open('data/bundletok.json'))
utok = {}
for l in open('data/units-tok.jsonl'):
    r = json.loads(l); utok[r['i']] = r['t']
pop = [json.loads(l) for l in open('data/population.jsonl')]
pop.sort(key=lambda u: u['s'])
pos = {u['i']: k for k, u in enumerate(pop)}
bygrp = collections.defaultdict(list)
for u in pop: bygrp[u['grp']].append(u)
# module graph between lazy-ESM groups: init name -> group; group -> deps (init calls in its wrapper body)
import re
rawrows = [json.loads(l) for l in open('/work/exp095-library-fingerprints/bundler-variation-and-boundaries/data/units/cc/min/cc-2.1.216/raw.jsonl')]
init2grp = {}; g = 0
for r in rawrows:
    if r['d'] == 0 and r['w']:
        m = re.match(r'(?:var |let |const )?([\w$]+)=', src[r['s']:r['s'] + 60])
        if m and r['wk'] == 'esm': init2grp[m.group(1)] = g
        g += 1
deps = collections.defaultdict(set); callers = collections.defaultdict(set)
for u in pop:
    if u['d'] == 1:
        m = re.fullmatch(r'([\w$]+)\(\);?', src[u['s']:u['e']])
        if m and m.group(1) in init2grp:
            deps[u['grp']].add(init2grp[m.group(1)]); callers[init2grp[m.group(1)]].add(u['grp'])
gtoks = collections.defaultdict(set)
lit = []
for l in open('/work/exp095-library-fingerprints/immutable-literals/verify/cc216-src-scored.jsonl'):
    r = json.loads(l); lit.append((r['startChar'], r['startChar'] + r['bytes'], r.get('claim'), r.get('bestPkg'), r.get('bestS'), r.get('pkg2'), r.get('pkg2S')))
lit.sort(); lits = [x[0] for x in lit]
def litclaim(s, e):
    k = bisect.bisect_right(lits, s) - 1
    if k >= 0 and lit[k][0] <= s and e <= lit[k][1]: return lit[k][2:]
    return None
def votes(toks, maxpk=4):
    v = collections.defaultdict(list)
    for t in toks:
        ps = idx.get(t)
        if not ps or len(ps) > maxpk: continue
        for p in ps: v[p].append(t)
    return sorted(v.items(), key=lambda kv: -len(kv[1]))
def short(t): return (t[:60] + '…') if len(t) > 60 else t
def card(u):
    s, e = u['s'], u['e']
    txt = src[s:e]
    g = bygrp.get(u['grp'], []) if u['grp'] is not None else []
    gs = min([x['s'] for x in g] + [s]); ge = max([x['e'] for x in g] + [e])
    gtxt = src[gs:ge]
    banner = [src[i:i+70].split('*/')[0] for i in range(len(gtxt)) if False]
    bpos = gtxt.find('/*!')
    out = [f"### id={u['i']} stratum={u['stratum']} s={s} e={e} bytes={u['bytes']} d={u['d']} grp={u['grp']} grpSpan={gs}-{ge} grpBytes={ge-gs} grpUnits={len(g)}"]
    if bpos >= 0: out.append('BANNER-IN-GROUP: ' + gtxt[bpos:bpos+80].split('*/')[0])
    lc = litclaim(s, e)
    if lc: out.append(f'literal-lane: claim={lc[0]} best={lc[1]}:{lc[2]} second={lc[3]}:{lc[4]}')
    head = txt if len(txt) <= 900 else txt[:700] + ' …[' + str(len(txt) - 900) + 'B]… ' + txt[-200:]
    out.append('TEXT: ' + head)
    toks = utok.get(u['i'], [])
    ranked = []
    for t in toks:
        ps = idx.get(t, [])
        bc = btok.get(t, [0, 0])
        ranked.append((len(ps) if ps else 99, -len(t), t, ps, bc))
    ranked.sort()
    out.append('TOKENS (npm pkgs | bundle src/vendor count):')
    ranked = [r for r in ranked if r[0] <= 30 or r[0] == 99]
    for n, _, t, ps, bc in ranked[:14]:
        out.append(f'  {short(t)!r}: {",".join(ps[:4]) if ps else "-"}{"+" + str(len(ps)-4) if len(ps) > 4 else ""} | {bc[0]}/{bc[1]}')
    out.append('STMT VOTES: ' + '; '.join(f'{p}={len(ts)}' for p, ts in votes(toks)[:5]))
    gt = set()
    for x in g:
        if x['i'] != u['i']: gt.update(utok.get(x['i'], []))
    gv = votes(gt)[:6]
    for x in g: pass
    out.append('GROUP VOTES: ' + '; '.join(f'{p}={len(ts)}[{", ".join(short(t) for t in ts[:3])}]' for p, ts in gv))
    def gv(gs_):
        tt = set()
        for gg in gs_:
            for x in bygrp.get(gg, []): tt.update(utok.get(x['i'], []))
        return '; '.join(f'{p}={len(ts)}' for p, ts in votes(tt)[:4])
    dg = deps.get(u['grp'], set()); cg = callers.get(u['grp'], set())
    out.append(f'DEP GROUPS ({len(dg)}) VOTES: ' + gv(dg))
    out.append(f'CALLER GROUPS ({len(cg)}) VOTES: ' + gv(cg))
    # product strings in group
    prod = [k for k in ('tengu_', 'Claude Code', 'claude.ai', 'CLAUDE.md', 'ANTHROPIC_', '.claude/', 'claude-code', 'Anthropic') if k in gtxt]
    out.append('GROUP PRODUCT STRINGS: ' + ','.join(prod))
    gl = sorted({t for t in gt if t.startswith('s:')}, key=lambda t: -len(t))[:6]
    out.append('GROUP LONG STRINGS: ' + ' | '.join(short(t[2:]) for t in gl))
    k = pos[u['i']]
    nb = []
    for j in (k - 2, k - 1, k + 1, k + 2):
        if 0 <= j < len(pop):
            x = pop[j]; tt = src[x['s']:x['e']]
            nb.append(f"  [{'-' if j < k else '+'}{abs(j-k)} d={x['d']} {x['e']-x['s']}B] " + (tt[:200] + '…' if len(tt) > 200 else tt))
    out.append('NEIGHBOURS:\n' + '\n'.join(nb))
    return '\n'.join(out)
items = [json.loads(l) for l in open(sys.argv[1])]
want = set(int(x) for x in sys.argv[3:])
with open(sys.argv[2], 'w') as f:
    for u in items:
        if want and u['i'] not in want: continue
        f.write(card(u) + '\n\n')
