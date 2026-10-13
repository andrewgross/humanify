# exp095 lane "negatives" — app/library ground truth for Claude Code 2.1.216 src/, plus exact-version Step B

## STATUS (2026-10-11)

- Task 1 DONE: 620-statement stratified random sample labelled (`labels.jsonl`, stratum S0–S3), blind second pass on
  124 (20%) — class agreement 124/124, package agreement 124/124. 38 traps (`traps.jsonl`). Review queue: 261 entries
  labelled as stratum `review-queue` (in `labels.jsonl` too, NEVER used in rates).
- Task 2 DONE: 22 exact-version app builds (Bun 1.4.0 --minify, dyn/imp/req, target node AND target bun = 134 builds);
  Step B (verified S3 config) on CC 2.1.216 src. Zero false claims on app bytes in the random sample.
- One label corrected after Step B: unit 28561 `figures` → `@inquirer/figures` (checked by hand: the module inlines
  isUnicodeSupported; figures@6 imports it). Blind pass did not include that unit.

## Population and span convention

Bundle: `claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js` (pure ASCII ⇒ byte offset == JS char offset;
`end` exclusive). Units = the cc_scan.mjs unit model (= `data/units/cc/min/cc-2.1.216/raw.jsonl` leaves): depth-0
non-wrapper statements / declarators, plus depth-1 statements inside lazy-ESM `var X=b(()=>{...})` bodies. CJS
factories (`Q(function(...))`, 5,052,733 B) are EXCLUDED. Population: 73,414 units, 15,280,719 B. `unit` = the raw.jsonl
`i`; this matches the lab/realworld convention so another lane can join on `unit` or on `[start,end)`.

## Sample (seed 95, `scripts/sample.py`)

| stratum | unit size | N      | bytes     | n   | lib | app | helper |
| ------- | --------- | ------ | --------- | --- | --- | --- | ------ |
| S0      | <100 B    | 56,859 | 834,509   | 150 | 36  | 114 | 0      |
| S1      | 100–999   | 13,785 | 4,377,939 | 180 | 33  | 146 | 1      |
| S2      | 1K–9,999  | 2,608  | 6,591,066 | 180 | 28  | 152 | 0      |
| S3      | ≥10K      | 162    | 3,477,205 | 110 | 11  | 99  | 0      |

Labels: `app` (incl. `fork_of` for CC's ink fork, pure-JS yoga port, commander-modelled help formatter, first-party
claude-agent-sdk code), `library:<pkg>`, `compiler-helper`, `bundler-runtime`, `unsure` (0 used).
@anthropic-ai/sandbox-runtime and @anthropic-ai/mcpb count as libraries (separately published deps).

## Result: library share of src/ bytes (stratified ratio estimator, stratified bootstrap B=20000 seed 95)

| class                     | share | 95% CI     | MB               |
| ------------------------- | ----- | ---------- | ---------------- |
| library                   | 14.5% | 11.2–18.1% | 2.22 (1.72–2.77) |
| library + compiler-helper | 14.7% | 11.4–18.3% | 2.24             |
| app                       | 85.3% | 81.7–88.6% | 13.04            |
| of which app forks        | 3.5%  | 1.3–6.0%   | 0.53             |

Top families (est KB): AWS SDK + smithy 560 (318–859), zod 384 (142–685), @anthropic-ai/_ 273 (112–481),
@azure/_ 225 (80–420), MCP SDK 148 (22–346), @inquirer/figures 100, lodash-es 99, jose 98, @bufbuild/protobuf 55,
growthbook 54, axios 52, socks5-server 48. Full table: `results-estimate.json`.

## Review queue (`immutable-literals/verify/cc-uncorroborated-2.1.216.txt`, 261 entries)

230 library, right package; 12 library, WRONG package (6 @smithy/core → @smithy/smithy-client ServiceException,
4 client-bedrock-runtime → client-bedrock, 2 protobufjs → @bufbuild/protobuf); 19 app (11,253 B) — all in `traps.jsonl`
or listed in `data/queue-labels.jsonl` with reasons. jose in CC is TWO copies: 4.15.x (≈19.94–20.02 MB offset) and 6.2.1–6.2.3.

## Versions (`scripts/versionscan.py`: CC src literals vs every pre-cut npm version's literals)

Declared in-bundle: @anthropic-ai/sdk 0.94.0, axios 1.15.2, msal-common 15.13.1, msal-node 3.8.1, AWS clients 3.936.0,
@azure/identity 4.10.1, acorn 8.15.0, fuse.js 7.0.0. Literal best-fit: MCP SDK 1.27.0–1.29.0, zod 3.25.76, marked 16.0.0,
jose 4.15.5–9 + 6.2.1–3, execa 7.1.1–8.0.1 region (old kill API), lru-cache ≥10.3 (tie), sandbox-runtime ≤0.0.66
(older than any npm version checked), bedrock-sdk 0.32.0, vertex-sdk 0.19.0, foundry-sdk 0.3.1/0.4.0,
google-cloud-sdk 0.0.4–0.0.6, @bufbuild/protobuf 2.12.x, @bufbuild/cel 0.5/0.6, growthbook 1.5.0 (built 1.6.1 – mismatch,
still recalled 100%), socks5-server 1.0.8, lodash-es 4.17.x, @typespec/ts-http-runtime 0.2.x. Ties = literals cannot split.

## Task 2: Step B on CC 2.1.216 src (`scripts/stepb_cc.py`, `results-stepb.json`, `results-stepb-rescored.json`)

Index A = lab min builds (3,092) + bvb exactver (12); B = A + this lane's 67 exact builds (target node);
C = B + 67 builds with `--target bun` (CC is a `bun build --compile` binary; with target node Bun hoisted the AWS
clients as eager ESM, so they never matched — target bun reproduces CC's lazy wrappers).

| index | src bytes claimed | sample recall (byte-wt, stratified) | right pkg | right family | false claims on app bytes (sample) |
| ----- | ----------------- | ----------------------------------- | --------- | ------------ | ---------------------------------- |
| A     | 811 KB            | 43.1%                               | 35.7%     | 38.5%        | 0                                  |
| B     | 1,647 KB          | 74.8%                               | 72.0%     | 74.8%        | 0                                  |
| C     | 1,851 KB          | 81.5%                               | 79.3%     | 81.4%        | 0                                  |

Claimed (index C, KB): zod 292, msal-common 119, jose 114, @anthropic-ai/sdk 108, client-bedrock 105, sandbox-runtime 103,
MCP SDK 92, msal-node 77, @bufbuild/protobuf 75, @azure/identity 73, axios 59, @smithy/core 42, marked 39, growthbook 38,
client-bedrock-runtime 37, lodash-es 32, @bufbuild/cel 30, … (+ ~150 KB of packages the 620 sample never hit:
zod-to-json-schema 21, jsonc-parser 17, fuse.js 16, @inquirer/figures 14, fflate 14, chokidar 11, diff 9, dom-mutator 6, …).
Every non-library claim:

- unit 25111 (3355959–3356252, 293 B) compiler-helper `__classPrivateFieldGet` (Stainless tslib copy) → claimed zod (A)
  / google-cloud-sdk (B, C). Twin 25110 (trap) same. Defensible as "carried by that package".
- trap/queue unit 41803/41804 (6705212–6705383, 171 B) CC ink-fork keypress tables → claimed `ink` (the fork kept ink's tables).
- unit 42003 (212 B) S2u shallow-props diff in CC's ink fork → claimed `ink` (same caveat; not in the random sample).
  Residual misses (C): client-bedrock-runtime 9%, mcpb 28%, @azure/identity 55%, MCP SDK 75%, client-sso/sts/core-client/bedrock-sdk 0%.

## Re-run

```
cd /work/exp095-library-fingerprints/negatives
python3 scripts/population.py && python3 scripts/sample.py          # seed 95
python3 scripts/emit_labels.py && python3 scripts/estimate.py        # labels.jsonl, results-estimate.json
(cd exactlab && python3 build.py --jobs 10 && TARGET=bun python3 build.py --jobs 10 && node scripts/label.mjs --jobs 12)
LAB_ROOT=$PWD/exactlab UNITS_OUT=$PWD/units/exact node ../bundler-variation-and-boundaries/scripts/scan.mjs --jobs 12
PYTHONHASHSEED=0 python3 scripts/stepb_cc.py                         # ~40 min, 12 procs
```

Blind pass ids: `blind/blind-ids.json` (seed 9520). Labels ledger: `data/pass1.tsv`, `data/traps.tsv`, `data/queue-labels.jsonl`.

## Unknown

- Exact versions where literals tie (lru-cache, smithy, p-map, figures, ansi-tokenize); sandbox-runtime's exact build.
- 36572 (computer-use tool schemas): labelled app, no installed package contains it — may be an internal workspace package.
- Per-package CIs are wide (1–13 sample items each); the overall share is the reliable number.
- CJS-factory bytes (5.05 MB, mostly vendor) are outside this population by design.
