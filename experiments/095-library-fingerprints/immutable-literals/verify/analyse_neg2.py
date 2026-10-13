"""VERIFY: out-of-index precision, deduplicated by (owner, literal set). Classifies every wrong (owner -> claim) pair
as COPY (owner really ships the claimed package's code: vendored/pre-bundled/fork/same-project/shared codegen helper)
or LOOKALIKE (independent code that shares literals). Classification table is by hand, from reading each pair's
statements (verify/neg-pairs.txt)."""
import json, collections, sys, hashlib
f = sys.argv[1] if len(sys.argv) > 1 else 'verify/neg-scored.jsonl'
seen = {}
src = [json.loads(l) for l in open('verify/neg-all.jsonl') if l.strip()]
scored = [json.loads(l) for l in open(f)]
assert len(src) == len(scored)
for s, r in zip(src, scored):
    if r['purity'] is not None and r['purity'] < 0.9: continue
    if r['pkg'] == '<unmapped>': continue  # no sourcemap owner: JSON tables etc.; owner unknown
    key = (r['pkg'], hashlib.sha1('\n'.join(sorted(s['lits'])).encode('utf-8','surrogatepass')).hexdigest())
    if key in seen: continue
    seen[key] = r
R = list(seen.values())
COPY = {('groq-sdk','@anthropic-ai/sdk'),('openai','@anthropic-ai/sdk'),('@ai-sdk/provider-utils','zod-to-json-schema'),('mcp-proxy','zod'),
 ('@modelcontextprotocol/server','@modelcontextprotocol/sdk'),('@modelcontextprotocol/client','@modelcontextprotocol/sdk'),('@anthropic-ai/sdk','qs'),
 ('openai','qs'),('encoding-sniffer','iconv-lite'),('@anthropic-ai/bedrock-sdk','@anthropic-ai/sdk'),('mime','mime-db'),('meow','yargs-parser'),
 ('meow','semver'),('@fastify/proxy-addr','proxy-addr'),('@inquirer/figures','figures'),('fast-string-truncated-width','get-east-asian-width'),
 ('ansi-styles','cli-boxes'),('@google/genai','tslib'),('punycode.js','punycode'),('meow','js-tokens'),('meow','glob'),('@inquirer/expand','iconv-lite'),
 ('openai','tslib'),('meow','indent-string'),('ora','yoctocolors'),('@ai-sdk/provider-utils','zod')}
cnt = collections.Counter(); pairs = collections.defaultdict(list)
for r in R:
    own = r['pkg']; cls = 'app' if own == '<app>' else ('in' if r['inIndex'] else 'out')
    cnt[cls+'-n'] += 1; cnt[cls+'-B'] += r['bytes']
    if r['claim'] is None: continue
    if r['claim'] == own: cnt[cls+'-right-n'] += 1; cnt[cls+'-right-B'] += r['bytes']; continue
    kind = 'copy' if (own, r['claim']) in COPY else 'lookalike'
    cnt[f'{cls}-wrong-{kind}-n'] += 1; cnt[f'{cls}-wrong-{kind}-B'] += r['bytes']
    pairs[(cls, kind, own, r['claim'])].append(r)
for k in sorted(cnt): print(k, cnt[k])
with open('verify/neg-pairs.txt', 'w') as o:
    for (cls, kind, own, c), rs in sorted(pairs.items(), key=lambda x: (x[0][1], -len(x[1]))):
        o.write(f'## [{cls}/{kind}] {own} -> {c}: {len(rs)} stmts, {sum(r["bytes"] for r in rs)} B\n')
        for r in rs[:6]: o.write(f'   n={r["n"]} tot={r["tot"]} top={r["top"][:2]} :: {r["head"][:160]}\n')
print(open('verify/neg-pairs.txt').read()[:200])
json.dump(dict(cnt), open('verify/neg-summary2.json', 'w'), indent=1)
