"""pass1.tsv (+ traps/queue tsv) -> labels.jsonl. Columns: unit id, label, pkg (for app: forked library, if a fork), reason.
Span convention: start/end are UTF-8 BYTE offsets (== JS char offsets; the bundle is pure ASCII) into the raw bundle
`file`, end exclusive; one row per leaf unit of bundler-variation-and-boundaries/cc_scan.mjs (unit index `unit`)."""
import json, sys
B = '/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js'
# versions declared by readers in the 2.1.216 bundle itself (banners, VERSION constants, inlined package.json)
DECLARED = {'@azure/msal-common': '15.13.1', '@azure/msal-node': '3.8.1', '@anthropic-ai/sdk': '0.94.0', 'axios': '1.15.2',
            '@aws-sdk/client-bedrock': '3.936.0', '@aws-sdk/client-bedrock-runtime': '3.936.0', '@aws-sdk/client-sts': '3.936.0',
            '@aws-sdk/client-sso': '3.936.0', '@aws-sdk/client-cognito-identity': '3.936.0', '@aws-sdk/nested-clients': '3.936.0',
            '@azure/identity': '4.10.1', 'acorn': '8.15.0', 'fuse.js': '7.0.0', 'chokidar': '4.0.3'}
def load(path, stratum_override=None):
    out = {}
    for l in open(path):
        if not l.strip() or l.startswith('#'): continue
        p = l.rstrip('\n').split('\t')
        out[int(p[0])] = (p[1], p[2], p[3] if len(p) > 3 else '')
    return out
sample = {u['i']: u for u in map(json.loads, open('data/sample.jsonl'))}
pop = {u['i']: u for u in map(json.loads, open('data/population.jsonl'))}
labs = load('data/pass1.tsv')
rows = []
for i, (lab, pkg, reason) in labs.items():
    u = sample.get(i) or pop[i]
    r = {'version': '2.1.216', 'file': B, 'unit': i, 'start': u['s'], 'end': u['e'], 'bytes': u['e'] - u['s'],
         'label': lab if lab != 'library' else f'library:{pkg}', 'pkg': pkg if lab == 'library' else None,
         'version_if_known': DECLARED.get(pkg) if lab == 'library' else None, 'fork_of': pkg if lab == 'app' and pkg else None,
         'reason': reason, 'stratum': u.get('stratum', 'random?'), 'depth': u['d'], 'group': u['grp']}
    rows.append(r)
import os
if os.path.exists('data/queue-labels.jsonl'):
    rows += [json.loads(l) for l in open('data/queue-labels.jsonl')]
rows.sort(key=lambda r: (r['stratum'] == 'review-queue', r['start']))
with open('labels.jsonl', 'w') as f:
    for r in rows: f.write(json.dumps(r) + '\n')
import collections
print(len(rows), collections.Counter(r['label'].split(':')[0] for r in rows))
