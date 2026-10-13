"""VERIFY: the Claude Code 2.1.216 src/ statements the recommended rule claims that are APP code (or app forks of a
library), found by reading every claim whose literal set is not corroborated by a real build (Jaccard < 0.5).
Verdicts by hand; prefix = the statement's first characters in the raw bundle."""
import json, sys
FP = {  # prefix -> (verdict, why)
 'function Lhr(':  ('app', 'React-compiled spinner component (glimmerIndex, shimmerColor, thinkingIntensity) -> claimed bowser'),
 'function mFu(':  ('app', 'raw stdin key handling, process.exit(130) -> bowser'),
 'function NYg(':  ('app', 'RGB -> xterm-256 colour conversion -> bowser'),
 'function k2y(':  ('app', 'same RGB -> xterm-256 conversion (second copy) -> bowser'),
 'var VUp=':       ('app', 'ANSI colour palette table {30:{r,g,b}...} -> bowser'),
 'function sir(':  ('app', 'debug pretty-printer -> react-reconciler'),
 'function sD_(':  ('app', 'transcript message normaliser (assistant/user, tool_use, tool_result) -> @anthropic-ai/sdk'),
 'function Tfd(':  ('app', 'transcript scan for a tool_use id -> @anthropic-ai/sdk'),
 'function D__(':  ('app', "Claude Code's marked renderer setup -> marked"),
 'function S$e(':  ('app', 'commander help configuration -> commander'),
 'function cJg(':  ('app', 'key-name char test; exact 2-way tie broken by the origin rule -> qs'),
 'function ozc(':  ('app', 'header-token char check (OTEL code); exact tie broken by origin rule -> undici'),
 'var mye=':       ('app', 'cron field ranges + weekday names -> date-fns'),
 'function _Ll(':  ('app', 'IPv4 parser; ip-address scored higher, origin rule picked axios'),
 'function _ab(':  ('app', 'key-binding filter (pageup/home/ctrl) -> ink'),
 'function Uhy(':  ('fork', "custom help formatter modelled on commander's formatHelp -> commander"),
 'function jnr(':  ('fork', "Claude Code's ink fork: text style transform (differs from ink Text.js) -> ink"),
 'var V7r,YYg':    ('fork', "Claude Code's ink fork: keypress sequence tables -> ink"),
}
if __name__ == '__main__':
    code = open('/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js').read()
    def load(p): return {r['startChar']: r for r in map(json.loads, open(p))}
    base = load('verify/cc216-src-scored.jsonl'); nos = load('verify/cc216-src-nosponge-scored.jsonl')
    orig = {r['startChar']: r for r in map(json.loads, open('results/cc-claims-2.1.216-plus-capvocab-origin.jsonl')) if r['kind'] == 'src-stmt'}
    agree = sum(1 for k, r in orig.items() if (r['claim'] or None) == (base[k]['claim'] or None))
    print('reproduction: my re-score agrees with the claimant on', agree, '/', len(orig), 'src statements')
    for name, S in (('as published', base), ('sponge files dropped', nos)):
        cl = [r for r in S.values() if r['claim']]
        print(f"{name}: {len(cl)} claims, {sum(r['bytes'] for r in cl)/1e6:.3f} MB")
    found = {}
    for k, r in base.items():
        t = code[k:k + 40]
        for p, v in FP.items():
            if t.startswith(p): found[p] = (k, r, nos[k])
    tot = {'app': [0, 0, 0], 'fork': [0, 0, 0]}
    for p, (v, why) in FP.items():
        k, r, n = found[p]
        tot[v][0] += 1; tot[v][1] += r['bytes']; tot[v][2] += 1 if n['claim'] else 0
        print(f"  [{v}] {p:16s} {r['bytes']:5d} B claimed={r['claim']:28s} nosponge={n['claim']}  ({why})")
    print('totals (count, bytes, still claimed without sponges):', tot)
