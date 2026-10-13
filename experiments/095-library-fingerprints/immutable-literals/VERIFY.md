# VERIFY: adversarial check of the "immutable literals" claims

Reviewer's re-measurement, 2026-10-11. Everything is in `verify/` (scripts, inputs, outputs), and the commands are at
the bottom. Nothing in the claimant's `scripts/`, `data/` or `results/` was changed.

## Verdict in one paragraph

The scorer reproduces exactly. The main finding holds: literals a minifier cannot rename are a strong signal for
recognising library code, and they work across bundlers. Two headline claims are overstated:

- **"Zero real false claims on Claude Code app code" is wrong.** It is true only on the hand-picked negative set.
  I read every Claude Code 2.1.216 src/ claim that real builds do not back up (160 statements). Under the recommended
  rule, **15 app statements (9.9 KB) and 3 statements from Claude Code's own forks of ink and commander (1.2 KB) are
  claimed as libraries.** That is 18 of 686 claims (2.6%), or about 1% of claimed bytes. The worst case is a 5 KB
  React spinner component claimed as `bowser`.
- **"93% of library bytes" holds only when the exact version is in the index.** On versions not in the index,
  measured by statement as Step B would run, byte recall is **81.9%**: 82.6% within an indexed major, 78.4% for a
  new major. By statement count it is **45–47%**.

Precision between libraries (naming the right package for library code) holds up well on new data. App-code
protection, the thing precision matters for, needs more guards before shipping.

## What I re-ran and how

| check                  | sample                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | what is independent about it                                                                                                                                                                                          |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Reproduce              | their `score.py` and `claim_q` on all 30,897 Claude Code 2.1.216 src/ statements                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | same inputs, so this tests the code                                                                                                                                                                                   |
| Out-of-index precision | **75 npm packages that are NOT in the index** (openai, groq-sdk, @google/genai, @mistralai/mistralai, @ai-sdk/anthropic, ai, @langchain/core, fastmcp, @anthropic-ai/bedrock-sdk, lodash, ramda, moment, luxon, dayjs, joi, yup, markdown-it, micromark, got, ky, koa, fastify, inquirer, ora, pg, mysql2, ioredis, cheerio, handlebars, winston, pino, rxjs, mobx, bcryptjs, …) plus all their dependencies, each bundled with **Bun 1.4.0 `--minify`** in imp / req / dyn modes. Every top-level statement is labelled by its sourcemap owner. 8,742 distinct out-of-index statements (12.2 MB), deduplicated by literal set. | packages, builds and labels are new; I wrote the labeller (`verify/build-label.mjs`). Weights are computed with the package genuinely absent, unlike their leave-package-out, which keeps the package in the weights. |
| Unseen-version recall  | 32 package versions that are NOT in the index, from indexed packages (axios 1.12.0, zod 3.24.1 and 4.1.5, jose 6.0.11 and 5.9.6, marked 17.0.1 and 15.0.12, MCP SDK 1.12.0, @anthropic-ai/sdk 0.60.0, date-fns 4.1.0, undici 7.10.0, hono 4.6.0, …), built the same way. 2,930 statements, 3.9 MB.                                                                                                                                                                                                                                                                                                                              | the index never holds the queried version                                                                                                                                                                             |
| Claude Code src/ audit | all 686 own-literal claims in 2.1.216 src/, checked against literal sets of the claimed package's statements in real builds (all lab builds, plus my Bun 1.4.0 builds of the pkgs-cc versions). I read every claim with Jaccard < 0.5 (160) and sampled the 0.5–0.8 band (101).                                                                                                                                                                                                                                                                                                                                                 | not selected by product strings                                                                                                                                                                                       |
| Recall weighting       | their lab spans, recomputed without the ×copies weight and with the purity filter on and off                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | —                                                                                                                                                                                                                     |

All runs use the recommended setting: `VARIANT=-plus-capvocab CLAIM=origin`, T=20, cover ≥ 0.9, margin 10.

## Results

### 1. Reproduction: exact

My re-score agrees with the claimant's published claim on **30,897 / 30,897** src/ statements (686 claims, 1.098 MB).

### 2. Claude Code app code claimed as a library (refutes "0 genuine")

Of 686 src/ claims:

- 425 have a statement with Jaccard ≥ 0.8 in a real build of the claimed package.
- 101 have Jaccard 0.5–0.8. I read all of them, and they are library code at a different version (zod, jose, axios, AWS).
- 160 have Jaccard < 0.5. Most of these are also genuine library code at a version the oracle builds lack: jose ×45, the MCP SDK's OAuth code, the AWS command classes. These are the false claims I found:

| statement (2.1.216)                              | bytes           | claimed as        | what it is                                                                                                                         |
| ------------------------------------------------ | --------------- | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `function Lhr(`                                  | 4,990           | bowser            | React-compiled spinner component (`glimmerIndex`, `shimmerColor`, `thinkingIntensity`)                                             |
| `function mFu(`                                  | 678             | bowser            | raw stdin key handling, `process.exit(130)`                                                                                        |
| `function NYg(`, `function k2y(`                 | 346 + 346       | bowser            | RGB to xterm-256 colour conversion (two copies)                                                                                    |
| `var VUp=`                                       | 422             | bowser            | ANSI colour palette table `{30:{r,g,b}…}`                                                                                          |
| `function sir(`                                  | 322             | react-reconciler  | debug pretty-printer                                                                                                               |
| `function sD_(`                                  | 735             | @anthropic-ai/sdk | transcript normaliser (`type==="assistant"`, `tool_use`, `tool_result`); claimed on a 4-bit margin through the origin tie-break    |
| `function Tfd(`                                  | 469             | @anthropic-ai/sdk | scans the transcript for a `tool_use` id                                                                                           |
| `function D__(`                                  | 445             | marked            | Claude Code's own marked renderer setup (app code that calls the library)                                                          |
| `function S$e(`                                  | 205             | commander         | help configuration (app code that calls the library)                                                                               |
| `function _ab(`                                  | 274             | ink               | key-binding filter                                                                                                                 |
| `var mye=`                                       | 174             | date-fns          | cron field ranges plus weekday names                                                                                               |
| `function cJg(`, `function ozc(`                 | 65 + 123        | qs, undici        | exact ties between 2 packages (js-yaml / bowser, whatwg-url / picomatch). The origin rule broke the tie and named a THIRD package. |
| `function _Ll(`                                  | 278             | axios             | IPv4 parser. `ip-address` scored higher, and the origin rule picked axios.                                                         |
| `function Uhy(`, `function jnr(`, `var V7r,YYg…` | 626 + 304 + 249 | commander, ink    | Claude Code's forks: a help formatter modelled on commander, and its own ink renderer and keypress tables                          |

**Totals: 15 app statements (9.9 KB) and 3 fork statements (1.2 KB).** 123 of the 686 src/ claims (149 KB) were
decided by the origin tie-break, not by the margin.

**A false-positive cause the claimant did not name: "sponge" files.** The index matches a query against its single
best FILE using containment, with no penalty for the file's size. Big pre-bundled files therefore contain almost
anything:

- `bowser/bundled.js`, 1,743 literals including core-js polyfills
- `date-fns/locale/cdn.js`, 11.4k literals
- `tr46/lib/mappingTable.json`

The look-alikes in the table above that are not protocol vocabulary are mostly these sponges plus number-heavy code,
and the 2-bit vocabulary cap does not touch numbers.

**Tested fix:** drop pre-bundled, minified, CDN and browser files from the index (807 of 25,215 files):

- 8 of the 15 app false claims go away: all 5 bowser claims, plus cJg, ozc and mye.
- Only those 8 claims are lost (686 → 678 claims, 1.098 → 1.091 MB).
- Unseen-version recall barely moves (81.9% → 81.4%).

The 7 that remain are app code that speaks a library's vocabulary or calls its API: the SDK transcript functions,
the marked and commander configuration, and the ink key handling.

### 3. Out-of-index library code (75 packages not in the index)

|                                                                                         | statements     | bytes         |
| --------------------------------------------------------------------------------------- | -------------- | ------------- |
| out-of-index statements (distinct literal sets)                                         | 8,742          | 12.15 MB      |
| claimed as some indexed package                                                         | 195 (2.2%)     | 277 KB (2.3%) |
| of which a real copy (vendored, pre-bundled, fork, same project, shared codegen helper) | 170            | 270 KB        |
| of which a look-alike (independent code)                                                | **25 (0.29%)** | 7.8 KB        |

**Copies** name the wrong package for code that really is library code:

- Stainless `__classPrivateFieldSet` helpers in openai and groq-sdk are claimed as @anthropic-ai/sdk (49 + more).
- zod-to-json-schema vendored inside @ai-sdk/provider-utils.
- qs vendored inside openai.
- meow's pre-bundled dependencies.

**Look-alikes**, each claimed at full cover:

| source code                                                        | claimed as |
| ------------------------------------------------------------------ | ---------- |
| luxon month names and `{year:0,month:1,…}`                         | date-fns   |
| formidable's WebSocket close codes 1000–1018                       | tr46       |
| bcryptjs base64 table                                              | bowser     |
| ramda / ioredis / core-util-is `[object Uint8ClampedArray]` checks | es-toolkit |
| `@ai-sdk` IPv4 private-range check                                 | bowser     |
| `new Set([301,302,303,307,308])`                                   | statuses   |
| mcp-proxy `Response.json({jsonrpc:"2.0",error:{code:-32603,…}})`   | MCP SDK    |
| superagent's parser registry                                       | mime-db    |
| markdown-it char-code switch, a 3-way exact tie broken by origin   | picomatch  |

The plain margin rule (no origin tie-break) gives 21 look-alikes and 141 copies instead of 25 and 170.

This 0.3% look-alike rate on library code agrees with their leave-package-out figure (458 / 91,794 = 0.5%).
The claimant's numbers are honest here. But at 0.1–0.3% of 30k src/ statements you get tens of false statements,
not zero, which matches section 2.

### 4. Recall when the exact version is not indexed

| set                                          | statements | byte recall | statement recall | wrong-package statements |
| -------------------------------------------- | ---------- | ----------- | ---------------- | ------------------------ |
| all 32 versions                              | 2,930      | **81.9%**   | 45.6%            | 3                        |
| unseen version, indexed major                | 2,260      | 82.6%       | 47.0%            | 3                        |
| new major                                    | 496        | 78.4%       | 38.3%            | 0                        |
| @anthropic-ai/sdk 0.60 (index has only 0.94) | 174        | 77.1%       | 47.7%            | 0                        |

Per package, recall ranges widely:

- high: marked 98%, yaml 97%, zod 94–96%, axios 89%
- low: undici 56%, date-fns 56%, jose 6.0.11 50%, chalk 41%, diff 7.0.0 43%, hono 21%

Their 93.3% uses an index that holds the very version that was built. That is the best case, not leakage. Their
"leave-version-out 89.8%" counts spans that were ALREADY claimed and is by span count, so it is not comparable to
this.

Precision between libraries holds on new versions: 3 wrong statements out of 2,930.

### 5. Things checked and upheld

- **Copy weighting does not inflate recall.**
  - Unique-span byte recall: 94.2% (purity ≥ 0.9), 92.5% (all spans).
  - Copies-weighted (as published): 93.3%, and 91.7% when spans with purity below 0.9 are included.
  - Span recall: 52.4%.
- **The negative-set labelling error is real.** `function nml(e){…"profile name is empty"…}` is
  `@anthropic-ai/sdk@0.94.0/core/credentials.js:46`. The env-federation statement `M$h` is in
  `lib/credentials/credential-chain.js`, also SDK code.
- **Bundler independence.** The same source-file index, never built with any bundler, scores my new Bun 1.4.0 builds
  at the levels above. The cross-bundler identity method (`extra.py`) compares the same (app, mode, file, part)
  occurrence across bundlers, which is sound.
- **Abstention on packages missing from the index.** 97.8% of out-of-index statements get no claim.

## Method flaws found

1. **The rule was designed on the test data.**
   - The vocabulary cap was added after seeing Claude Code negative false claims.
   - The origin tie-break was added after seeing MSAL recall in Claude Code.
   - T, cover and margin were picked from a grid scored on the same lab spans and Claude Code sets they report.
   - There is no held-out set. On my held-out data the rule generalises roughly, but not to "zero false claims".
2. **The app-negative evidence is weak.**
   - The lab's `<app>` code is one synthetic function per entry with a unique string, so "appFP 1/2411" says nothing
     about real app code.
   - The Claude Code negatives were selected by product strings (the claimant notes this).
   - The statement-level "99.4% precision" is wrong-package-among-library-statements, not app protection.
3. **Statement-level recall is reported by bytes only.** By statement count, lab Step B recall is **28.9%** (own
   literals; `stepb-plus-capvocab-origin.txt`). Most small statements stay unclaimed, which matters for clean
   boundaries in src/.
4. **The origin tie-break turns exact ties into claims.** When 2–3 packages tie exactly, it names whichever has the
   smallest file, sometimes a package that was not even in the top 2 (cJg, ozc, markdown-it → picomatch). It decides
   123 / 686 Claude Code src/ claims.
5. **Containment against the best file, with no size penalty.** This is what makes sponge files universal matchers.
   It was not in the claimant's risk list.
6. **Leave-package-out does not recompute weights** (acknowledged). My out-of-index test does, and it agrees within
   a factor of about 2.

## Corrected verdict

Literal fingerprints work and are bundler-independent. With the recommended rule:

- **Naming accuracy:** for library code the index covers, about 93% of bytes when the exact version is indexed and
  about 82% when it is not. Wrong-package errors are rare (≤0.1%).
- **Abstention:** library code that is not indexed is left unclaimed 97.8% of the time. 0.3% is claimed by a
  look-alike and 1.9% is named after a vendored or copied package.
- **App code is NOT fully protected.** In Claude Code src/, about 2.6% of claims (18 statements, 11 KB) are app code
  or app forks.

Before claims are allowed to hide src/ code:

- Drop pre-bundled, minified and CDN dist files from the index. This removes 8 of the 15 at no measurable recall cost.
- Refuse exact or near ties instead of breaking them by file size, or require the tie-break winner to be in the top 2.
- Require corroboration (a consistent neighbour run, or structure) for claims under about 40 bits.
- Build a real app-negative set. Every src/ statement whose claim is not corroborated by a real build is a ready-made
  review queue (`verify/cc-uncorroborated-*.txt`).

The headline "Yes" holds for vendor naming. For Step B it holds only with these guards added and re-measured.

## Files and re-run

```
verify/build-label.mjs      Bun 1.4.0 minified builds + sourcemap-owner labels per top-level statement + literals (their lits.mjs)
verify/score_verify.py      their score.run + recognise.claim_q on any statement set; NOSPONGE=1 drops pre-bundled dist files
verify/analyse_neg2.py      out-of-index precision, copy vs look-alike (hand table: verify/copy-pairs.json); -> verify/neg-pairs.txt
verify/analyse_ver.py       unseen-version recall                    -> verify/ver-summary.json
verify/cc_oracle.py         corroborate Claude Code src/ claims with real builds -> verify/cc-oracle-<v>.json, cc-uncorroborated-<v>.txt
verify/cc_fp_list.py        the hand-audited false claims, reproduction check, sponge effect
verify/rule_compare.py      plain vs origin rule on the independent sets
verify/neg/list.txt, verify/ver/specs.txt   the package samples
```

```bash
cd /work/exp095-library-fingerprints/immutable-literals/verify
# installs (network): neg/ = 75 out-of-index packages, ver/p/<spec>/ = 32 unseen versions, ccpk -> ../pkgs-cc
(cd neg && npm install --ignore-scripts --no-audit --no-fund --legacy-peer-deps $(tr '\n' ' ' < list.txt))
while read spec cat; do d=ver/p/$(echo $spec | tr / _); mkdir -p $d; (cd $d && echo '{"private":true}' > package.json && npm install --ignore-scripts --no-audit --no-fund --legacy-peer-deps $spec && echo ${spec%@*} > list.txt); done < ver/specs.txt
# builds + labels (~5 min); the 4 packages with optional deps need EXT=… (see neg/part.retry.log); MCP SDK 1.12 uses list2.txt subpaths
split -n l/16 -d neg/list.txt neg/part.; for f in neg/part.??; do node build-label.mjs neg $f $f.jsonl > $f.log 2>&1 & done; wait
for d in ver/p/*; do node build-label.mjs $d $d/list.txt $d/out.jsonl > $d/build.log 2>&1 & done; wait
mkdir -p ccpk && ln -sfn ../../pkgs-cc/node_modules ccpk/node_modules && node build-label.mjs ccpk ccpk/list.txt ccpk/out.jsonl
cd .. && python3 verify/prep.py   # -> verify/neg-all.jsonl, ver-all.jsonl, cc216-src.jsonl
VARIANT=-plus-capvocab CLAIM=origin tools/venv/bin/python verify/score_verify.py verify/neg-all.jsonl verify/neg-scored.jsonl
VARIANT=-plus-capvocab CLAIM=origin tools/venv/bin/python verify/score_verify.py verify/ver-all.jsonl verify/ver-scored.jsonl
VARIANT=-plus-capvocab CLAIM=origin tools/venv/bin/python verify/score_verify.py verify/cc216-src.jsonl verify/cc216-src-scored.jsonl
NOSPONGE=1 VARIANT=-plus-capvocab CLAIM=origin tools/venv/bin/python verify/score_verify.py verify/cc216-src.jsonl verify/cc216-src-nosponge-scored.jsonl
tools/venv/bin/python verify/analyse_neg2.py; tools/venv/bin/python verify/analyse_ver.py
tools/venv/bin/python verify/cc_oracle.py 2.1.216; tools/venv/bin/python verify/cc_fp_list.py
VARIANT=-plus-capvocab tools/venv/bin/python verify/rule_compare.py
```
