"""Step B on Claude Code: index = the lab's library fingerprints; query = Claude Code's raw bundle.

Scores:  negatives (100 app module groups, ~0.9 MB) -> any byte flagged is a false positive
         positives (214 declared-identity vendor factories) -> recall, right package
Census:  library bytes found per region (CJS factory = vendor/, lazy-ESM = src/, eager tail) and per package, with
         string corroboration: a flagged statement whose long literals are known for that package in the index.
usage: python3 scripts/cc_stepb.py  -> results/stepb-cc.json, results/stepb-cc-examples.txt
"""
import json, sys, collections, random
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, index_from_builds, detect, score, summarize, ROOT, UNITS, NONLIB
from load import build_dirs

LABBUILDS = [b for b in build_dirs() if '/min/' in b and not b.startswith('cc/')]
VERS = ['2.1.216', '2.1.198']
BUNDLE = '/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-{}/binary-decompiled/src/entrypoints/index.js'


def regions(ver):
    rows = [json.loads(l) for l in open(f'{UNITS}/cc/min/cc-{ver}/raw.jsonl')]
    wk = {r['i']: r['wk'] for r in rows if r['w']}
    reg = {}
    seen_wrapper = False
    pending = []
    for r in rows:
        if r['w']:
            for s in pending:
                reg[s] = 'src (lazy-ESM, hoisted part)' if wk[r['i']] == 'esm' else 'between factories'
            pending = []
            continue
        if r['d'] == 1:
            reg[r['s']] = 'vendor (CJS factory)' if wk[r['p']] == 'cjs' else 'src (lazy-ESM, init body)'
        else:
            pending.append(r['s'])
    for s in pending:
        reg[s] = 'eager tail'
    return reg


def main():
    out = {}
    ex = open(ROOT + '/results/stepb-cc-examples.txt', 'w')
    LABHLJS = {'javascript', 'python', 'bash', 'json', 'html, xml', 'xml', 'css', 'typescript', 'go', 'rust', 'sql', 'c++', 'c', 'actionscript', 'arduino'}
    pos_rows = {v: [json.loads(l) for l in open(f'/work/exp095-library-fingerprints/lab/realworld/cc-{v}/positives.jsonl')] for v in VERS}
    for level in ('h3',):
        idx = index_from_builds(LABBUILDS, level)
        print(f'index level={level} builds={len(LABBUILDS)} fn={len(idx["fn"])} stmt={len(idx["stmt"])} str={len(idx["str"])}', flush=True)
        for ver in VERS:
            units = load_units(f'cc/min/cc-{ver}/raw')
            reg = regions(ver)
            text16 = open(BUNDLE.format(ver), encoding='utf8').read().encode('utf-16-le')
            snip = lambda u, n=200: text16[2 * u['s']:2 * min(u['e'], u['s'] + n)].decode('utf-16-le', 'replace')
            idx_pkgs = set()
            for ps in idx['fn'].values():
                idx_pkgs |= ps
            known_member = {}
            for r in pos_rows[ver]:
                if r['pkg'] == 'highlight.js':
                    known_member[r['textPath']] = (r['member'] or '').lower() in LABHLJS
                else:
                    known_member[r['textPath']] = r['pkg'] in idx_pkgs
            # map positive label idx -> known?
            labs = json.load(open(f'{UNITS}/cc/min/cc-{ver}/raw.labels.json'))
            for strategy, nmin, mc in [('S1', 30, 0), ('S3', 30, 0), ('S3', 30, 3), ('S3', 30, 5), ('S3', 50, 3), ('S3', 80, 0), ('S3', 20, 3), ('S4', 30, 3), ('A3', 30, 0)]:
                if True:
                    pred = detect(units, idx, nmin=nmin, strategy=strategy, mincontent=mc)
                    r = score(units, pred, pkg_ok=lambda t, p: t == p or (t, p) == ('highlight.js', 'highlight.js'))
                    s = summarize(r)
                    # census
                    byreg = collections.Counter()
                    regtot = collections.Counter()
                    bypkg = collections.Counter()
                    corro = collections.Counter()
                    for u, p in zip(units, pred):
                        b = u['e'] - u['s']
                        rg = reg.get(u['s'], '?')
                        regtot[rg] += b
                        if p:
                            byreg[rg] += b
                            bypkg[p] += b
                            if u['truth'][0] == '<none>':
                                known = [idx['str'].get(h) for h in u['ls']]
                                if not u['ls']:
                                    corro['no long strings'] += b
                                elif any(k and p in k for k in known):
                                    corro['a literal known for that package'] += b
                                elif any(k for k in known):
                                    corro['literals known only for OTHER packages'] += b
                                else:
                                    corro['literals unknown to the index'] += b
                    s['flaggedMB by region'] = {k: f'{byreg[k]/1e6:.2f} of {regtot[k]/1e6:.2f}' for k in regtot}
                    s['top packages KB'] = [(k, round(v / 1e3)) for k, v in bypkg.most_common(40)]
                    s['corroboration (unknown-truth flagged bytes) KB'] = {k: round(v / 1e3) for k, v in corro.items()}
                    # positives the index could know (package in catalog; highlight.js only the 14 grammars the lab bundles)
                    kp = collections.Counter()
                    for u, p in zip(units, pred):
                        t = u['truth']
                        if t[0] in NONLIB or t[0] == '<none>':
                            continue
                        member = (t[3] or '').lower()
                        known = (member in LABHLJS) if t[0] == 'highlight.js' else (t[0] in idx_pkgs)
                        k = 'known' if known else 'unknown'
                        kp[k + 'Bytes'] += u['e'] - u['s']
                        if p:
                            kp[k + 'Found'] += u['e'] - u['s']
                            kp[k + 'Right'] += (u['e'] - u['s']) if p == t[0] else 0
                    s['positives the index could know: recall%'] = round(100 * kp['knownFound'] / kp['knownBytes'], 1) if kp['knownBytes'] else None
                    s['positives the index could know: KB'] = round(kp['knownBytes'] / 1e3, 1)
                    s['positives the index could know: right pkg%'] = round(100 * kp['knownRight'] / kp['knownFound'], 1) if kp['knownFound'] else None
                    s['positives NOT in catalog: flagged%'] = round(100 * kp['unknownFound'] / kp['unknownBytes'], 1) if kp['unknownBytes'] else None
                    key = f'{level} | {ver} | {strategy} nmin={nmin} mincontent={mc}'
                    out[key] = s
                    print(key, json.dumps({k: v for k, v in s.items() if k not in ('top packages KB',)}), flush=True)
                    # examples: every flagged app byte, and a random sample of flagged unknown-truth statements
                    if (strategy, nmin, mc) in (('S3', 30, 3), ('S3', 30, 0)) and level == 'h3':
                        ex.write(f'\n\n######## {key}\n')
                        for u, p in zip(units, pred):
                            if p and u['truth'][0] == '<app>':
                                ex.write(f'APP FLAGGED as {p}: {snip(u)!r}\n')
                        rnd = random.Random(7)
                        cand = [(u, p) for u, p in zip(units, pred) if p and u['truth'][0] == '<none>' and reg.get(u['s'], '').startswith('src')]
                        for u, p in rnd.sample(cand, min(60, len(cand))):
                            ex.write(f'[{p}] {reg.get(u["s"])}: {snip(u, 240)!r}\n')
    json.dump(out, open(ROOT + '/results/stepb-cc.json', 'w'), indent=1)


main()
