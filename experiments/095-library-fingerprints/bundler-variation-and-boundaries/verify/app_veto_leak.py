"""VERIFY: do the combo queries' APP statements/functions already sit in the headline index as <nonlib> (a veto the
detector could only have because the lab labelled them)? usage: python3 verify/app_veto_leak.py"""
import sys, collections
sys.path.insert(0, '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts')
from stepb import load_units, index_from_builds
from load import build_dirs
mins = [b for b in build_dirs() if '/min/' in b]
idx = index_from_builds([b for b in mins if b.split('/')[0] in ('bun-1.3.14', 'bun-1.4.3', 'esbuild-0.28.2') and 'combo' not in b], 'h3')
c = collections.Counter()
for b in [b for b in mins if b.startswith('bun-1.4.0/') and '/combo' in b]:
    for u in load_units(b):
        if u['truth'][0] != '<app>': continue
        c['appStmts'] += 1; c['appBytes'] += u['e'] - u['s']
        if u['h3'] in idx['stmt']:
            c['stmtInIndex'] += 1; c['stmtInIndexBytes'] += u['e'] - u['s']
            if '<nonlib>' in idx['stmt'][u['h3']]: c['stmtVetoed'] += 1; c['stmtVetoedBytes'] += u['e'] - u['s']
        for f in u['fns']:
            if f[2] >= 30:
                c['appFns30'] += 1
                if f[5] in idx['fn']: c['fnInIndex'] += 1; c['fnInIndexAsLib'] += '<nonlib>' not in idx['fn'][f[5]]
print(dict(c))
