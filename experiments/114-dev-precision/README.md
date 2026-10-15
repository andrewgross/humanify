# exp114: dev precision under the shipped defaults

## STATUS (2026-10-14)

DONE on branch `exp114-dev-precision` (off main `f568e5bc`). Three new `[library.discovery]` keys, all default
OFF, test-first (`discovery_test.rs`: `context_presence_*`, `shadow_ties_*`, `presence_fence_*`;
`library_config_test.rs`). Gate: ALL 13 STAGES PASSED at every commit. Library matching is off by default, so the
pipeline's default output cannot move (warm neutrality below). Every number is scorer `exp096-score/6`, structural
rule, index k2-l12m4 (Bun/esbuild) / step4-wr (webpack/rollup), the shipped config plus the key named.

- **`shadow_ties = true` alone: clean on every dev set AND every test set** (the safe minimum flip).
- `context_presence = 1.5` (+ `context_presence_slack = 16384`) ALONE: best dev gains, but test-bun F0.5 -0.52
  (precision -0.98): the code it frees is reclaimed by the origin, which the test truth credits to the shipper.
  With **`presence_fence = true`** (name neither) test-bun turns +0.12, but test-webpack is -0.04 (cpf).
- **The three together (`allf` = context_presence 1.5 / 16384 + shadow_ties + presence_fence) are clean on all nine
  sets** (every F0.5 >= shipped, precision up everywhere it moves): held-out 90.18 -> 90.29, task-master 87.57 ->
  87.94, apps 96.63 -> 96.78, test-bun 74.07 -> 74.29, test-esbuild 79.42 -> 79.53, test-webpack 65.93 -> 66.02,
  test-rollup 57.74 -> 57.96. **Recommended default: all three.**
- **The 99.8% precision bar is NOT reached on any set but Claude Code.** Held-out goes 99.09 -> 99.23 at best;
  task-master 95.69 -> 96.25. What is left is decomposed below; most of it is either a truth-model question
  (exp112's lane) or needs index-side evidence (a vendored copy's file path) that no runtime rule has.

## Totals first (P / R / **F0.5**)

| config | heldout | esbuild-app | wrangler | cc | apps-esbuild | test-bun | test-esbuild | test-webpack | test-rollup |
|---|---|---|---|---|---|---|---|---|---|
| shipped (exp109) | 99.09 / 66.33 / **90.18** | 95.69 / 65.36 / **87.57** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.53 / 89.70 / **96.63** | 80.58 / 55.99 / **74.07** | 83.61 / 66.16 / **79.42** | 85.84 / 34.19 / **65.93** | 76.89 / 28.92 / **57.74** |
| cp | 99.14 / 66.39 / **90.24** | 96.21 / 65.47 / **87.95** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.64 / 89.71 / **96.72** | 79.61 / 56.40 / **73.55** | 83.72 / 66.38 / **79.56** | 85.83 / 34.16 / **65.89** | 76.90 / 28.92 / **57.74** |
| cp8 | 99.14 / 66.39 / **90.24** | 96.26 / 65.47 / **87.98** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.71 / 89.72 / **96.77** | - | - | - | - |
| st | 99.16 / 66.38 / **90.25** | 95.68 / 65.34 / **87.55** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.61 / 89.79 / **96.71** | - | - | - | - |
| st2 | 99.17 / 66.39 / **90.26** | 95.73 / 65.39 / **87.60** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.62 / 89.72 / **96.70** | 80.71 / 56.08 / **74.20** | 83.69 / 66.22 / **79.49** | 86.04 / 34.29 / **66.09** | 77.18 / 29.03 / **57.95** |
| both | 99.22 / 66.45 / **90.31** | 96.20 / 65.45 / **87.94** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.72 / 89.80 / **96.80** | - | - | - | - |
| both2 | 99.23 / 66.45 / **90.32** | 96.25 / 65.50 / **87.99** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.73 / 89.73 / **96.79** | 79.73 / 56.49 / **73.67** | 83.79 / 66.44 / **79.63** | 86.03 / 34.25 / **66.06** | 77.19 / 29.03 / **57.96** |
| cpf | 99.15 / 66.32 / **90.22** | 96.20 / 65.36 / **87.91** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.64 / 89.69 / **96.71** | 80.75 / 55.99 / **74.19** | 83.68 / 66.17 / **79.47** | 85.83 / 34.16 / **65.89** | 76.90 / 28.92 / **57.74** |
| allf | 99.24 / 66.35 / **90.29** | 96.24 / 65.39 / **87.94** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.73 / 89.71 / **96.78** | 80.88 / 56.04 / **74.29** | 83.75 / 66.19 / **79.53** | 86.01 / 34.20 / **66.02** | 77.19 / 29.03 / **57.96** |

Wrong-family bytes, shipped -> allf: held-out 185,893 -> 154,754, task-master 144,685 -> 125,529, apps-esbuild
342,936 -> 295,542; wrangler 5,652 unchanged. Test: app claimed 0 on every set under every key (test labels commit their
summaries only).

`cp` = context_presence 1.5 / slack 16384; `cp8` = slack 8192; `st` = shadow_ties as first written (commit
`c82b0cd5`); `st2` = shadow_ties with the origin-size condition (`ce110cb7`, the version on the branch); `both*` =
cp + st*; `cpf` = cp + presence_fence; `allf` = cp + st2 + presence_fence (`2333caed`). Wrangler and Claude Code
do not move under any key.

## 1. Where the remaining wrong-family bytes are (shipped default, before any key)

Wrong-family bytes: held-out 185,927; task-master 144,730; wrangler 5,652; apps-esbuild 342,936. To pass the bar,
held-out would need <= ~40 KB, task-master <= ~6 KB, wrangler <= ~270 B, apps <= ~46 KB.

**By whether the claimed package is in the bundle at all** (`pkgpresence.py`): the wrong bytes of packages ALL of
whose claims in that bundle are wrong — the package is not there — are 71% of held-out (132 KB), 95% of
task-master (138 KB), 99% of wrangler. So the dominant error is a package named in a bundle that does not contain
it, not overreach of a present package.

**By the rule that made the claim and the index evidence** (`wrongmech.py`, structural rule emulated):

| set | seed: tie lost | seed: truth outscored | seed: truth holds no print (version not indexed) | seed: truth not indexed | seed: no print (queue) | declared / banner | propagated onto a unit no package holds | propagated, other |
|---|---|---|---|---|---|---|---|---|
| held-out | 24.8 K | 0 | 26.7 K | 17.4 K | 8.3 K | 11.8 K | 71.7 K | 25.2 K |
| task-master | 25.2 K | 0 | 32.6 K | 2.5 K | 0.1 K | 0 | 17.9 K | 66.4 K (the @vscode/tree-sitter-wasm DOWN: 62.6 K) |
| wrangler | 0 | 0 | 5.5 K | 0 | 0 | 0 | 0.1 K | 0 |
| apps-esbuild | 10.2 K | 54.3 K | 137.7 K | 56.1 K | 0.1 K | 0 | 54.6 K | 29.8 K |

**By mechanism** (top pairs read by hand, the rest by the declared-dependency relation; approximate):

| mechanism | held-out | task-master | wrangler | apps |
|---|---|---|---|---|
| A. a SHIPPER named for its dependency's code (sponge / pre-bundled copy / fork: ncu<-zod, @chromatic-com/storybook<-zod, @vscode/tree-sitter-wasm<-web-tree-sitter, crossws<-ws, @prisma/fetch-engine<-picomatch, @mastra/core<-ai, @livekit/components-react<-rxjs, @antv/vendor<-d3-*, @langchain/core<-uuid / zod-to-json-schema, @rspack/core<-tinypool) | 88 K | 137 K | 0 | 264 K |
| B. the ORIGIN named, truth credits the shipper of an unmarked copy (valibot<-rolldown, eastasianwidth<-@clack/core, @noble/curves<-openpgp, es-module-lexer<-vite, confbox<-jsonc-parser) | 30 K | 7 K | 5.6 K | 69 K |
| C. shared generated code (wasm-bindgen glue: @matrix-org/matrix-sdk-crypto-wasm 33.6 K; Go `wasm_exec`: @netlify/redirect-matcher 7.8 K; Angular compiled templates: @angular/* <- @ng-bootstrap 16 K) | 61 K | 1 K | 0 | 9 K |
| D. a package reader's declaration (sharp's reader on semver code) | 6.8 K | 0 | 0 | 0 |

Class B is the truth question exp110 hit and exp112 is working on (scorer 7: license-collection origins): the
claim names the code's real origin; no matcher rule should remove it. Wrangler's whole residual is class B, which
is why no key moves it.

## 2. What was tried in the lab first (seed-level simulations over every claim; `seedlab.py`, `simulate.py`, `proplab.py`)

Break-even: removing 1 wrong byte is worth ~2.6 good bytes on held-out, 2.2 on task-master, 0.7 on wrangler, 3.4
on apps (F0.5). Refuted before any Rust:

- **Tie-break by bundle presence** (give a tie to the family with the most unique seeds): good->wrong 234 KB on
  held-out (katex/mermaid: mermaid inlines katex, the app bundles both).
- **Tie-break by specificity** (smaller index module wins): good->wrong 115-261 KB (openpgp/@noble, prettier/
  @angular/compiler: the app bundles the big shipper and the truth credits it).
- **Carrier presence** (drop a package that co-holds a present package's prints): 1.6-2.1 MB good lost.
- **Neighbourhood vote** (drop a seed whose neighbours are another family it co-holds): 55-274 KB good lost per set.
- **Anchor rule** (a package needs one unambiguous seed >= N nodes): N=100: held-out 212 KB good for 44 KB wrong.
- **Higher contested_cover** on propagation (0.3-0.5): removes the tree-sitter DOWN (62.6 KB) but costs apps 57 KB,
  wrangler 17 KB good.
- **Copy-ambiguity by declared dependency** (the brief's idea 1): the registry metadata covers too few of the
  claimed packages (ncu does not declare zod; @vscode/tree-sitter-wasm, @prisma/fetch-engine, @mastra/core have no
  manifest in the caches), and where it does apply the claim it removes is often class B. Not implemented.
- **Index module "aggregate" share** (`aggregate.py`: per index module, the share of its prints another family
  holds in a smaller module): a sponge's dist reads only 3-26% copy, because its copies are of versions the index
  does not hold. A weak signal; the strong one is the FILE PATH of the vendored copy (`dist/_chunks/libs/ws.mjs`,
  `compiled/tinypool/`, `build/chunks/typescript-*.cjs`), which the unit table records but a run cannot see
  (`UnitHit` carries owner and module bytes only).

What survived: a package-level reading of SIZE (context presence) and of TIES (shadow ties).

## 3. The keys

**`shadow_ties`** (`presence()` / `shadow_ties()` in `libdetect/discovery.rs`). Over the bundle's index seeds: a
package P whose EVERY index seed is a tie (the structural rule would accept another family too) while some rival
family Q has seeds of its own (accepted for Q alone) in the bundle is Q's shadow: P's tied seeds become Q's, its
other seeds are dropped. A rival counts only where it holds the tied code in an index module no bigger than P's
(`st2`): the origin is the smaller context; without that condition (`st`) a sponge with its own copy-seeds
absorbed a real origin's ties (chalk -> npm-check-updates on task-master, d3-path -> mermaid on held-out).
Declared/banner seeds pass, and a package with one keeps everything.

**`context_presence` / `context_presence_slack`**. A package every one of whose index seeds sits in a module group
(a CommonJS factory or lazy module; scope-hoisted units are not judged) whose bytes x factor + slack are still below
the SMALLEST index module in which the package holds that unit's prints is no seed: the package's copy of this code
lives in a dist bigger than the module the bundle shows it in, and nowhere does the bundle show the package at its
own size. Dropped seeds leave the code open, and the origin usually reclaims it (rxjs, zod on held-out: recall UP).

**`presence_fence`**: what the two keys drop is claimed by a sentinel during the fixpoint and the sentinel's claims
are removed after — the code is named by no one (the brief's "name neither"), instead of reclaimed.

## 4. Hand-reads of removed / renamed claims

15 claims the keys removed or renamed, drawn with `handread.py` (one per (bundle, old package, new package) group,
random seeds 7 / 3; st2 = 8, cp = 7 incl. 3 from apps). "Truth file" is the source the bundle's own map points at.

| # | bundle | was | now | truth file | really wrong? |
|---|---|---|---|---|---|
| 1 | held-out codex | @antv/vendor (seed) | unclaimed | d3-dispatch 3.0.1 src/dispatch.js | yes: d3-dispatch's `dispatch` prototype; @antv/vendor re-bundles d3 |
| 2 | held-out task-master | @remix-run/web-fetch (seed) | node-fetch (right) | node-fetch 3.3.2 src/errors/base.js | yes: node-fetch's FetchBaseError, web-fetch is a fork |
| 3 | held-out codex | @antv/vendor (seed) | d3-dsv (right) | d3-dsv 3.0.1 src/dsv.js | yes: d3-dsv's formatDate |
| 4 | held-out appium | @langchain/core (seed) | uuid (right) | uuid 14.0.2 dist-node/parse.js | yes: uuid's parse ("Invalid UUID") |
| 5 | held-out appium | @langchain/core (queue seed) | unclaimed | uuid 14.0.2 dist-node/version.js | yes: uuid's version() |
| 6 | held-out renovate | @remix-run/web-fetch (seed) | node-fetch (right) | node-fetch 3.3.2 src/errors/base.js | yes (as 2) |
| 7 | held-out task-master | @vscode/tree-sitter-wasm (seed) | web-tree-sitter (right) | web-tree-sitter 0.25.10 tree-sitter.js | yes: web-tree-sitter's predicate checks |
| 8 | held-out wrangler | @vitest/utils (seed) | pathe (right) | pathe 2.0.3 dist/shared/pathe.*.mjs | yes: pathe's normalizeString |
| 9 | task-master (esbuild) | @chromatic-com/storybook (seed) | @anthropic-ai/claude-agent-sdk (right) | agent-sdk 0.1.77 sdk.mjs (origin zod) | yes: zod's ZodNaN inside agent-sdk; storybook is not in the app |
| 10 | held-out task-master | npm-check-updates (seed) | unclaimed | mcp-proxy 6.7.19 dist/startStdioServer-*.mjs (origin zod) | yes: zod 4's ZodObject in mcp-proxy's chunk; ncu is not in the app |
| 11 | apps puppeteer | @livekit/components-react (seed, 7 KB) | unclaimed | puppeteer-core lib/third_party/rxjs/rxjs.js (origin rxjs) | yes: rxjs's Subscriber, vendored by puppeteer |
| 12 | apps puppeteer | @livekit/components-react (down) | puppeteer-core (right) | as 11 | yes |
| 13 | apps opencomic | html-dom-parser (seed) | htmlparser2 (right) | htmlparser2 12.0.0 src/Tokenizer.ts | yes: htmlparser2's CharCodes enum |
| 14 | held-out lighthouse | @livekit/components-react (module, 2 B) | puppeteer-core | `<unmapped>` | not judgeable (a 2-byte name) |
| 15 | held-out task-master | zod (down, origin: RIGHT) | @modelcontextprotocol/ext-apps (WRONG) | mcp-proxy chunk (origin zod) | NO — a regression of `cp` without the fence: ncu's seeds dropped, ext-apps (another zod carrier) propagated into the freed code |

13 of 15 were really wrong, 1 unjudgeable, 1 is a NEW wrong claim `cp` introduced (case 15) — exactly the failure
mode `presence_fence` closes (the freed region is named by no one), and the reason `cp` alone is not recommended.

## 5. Recommendation

1. **Flip `shadow_ties = true`** (clean alone on all nine sets).
2. **Flip `context_presence = 1.5`, `context_presence_slack = 16384`, `presence_fence = true` with it** (`allf`:
   clean on all nine; never flip context_presence without the fence — test-bun -0.52).
3. Not reached: the 99.8% bar. The remaining wrong mass is (A) shippers named for an origin's code where the
   bundle shows the shipper at its own size (ncu's own supports-color chunk keeps it present on task-master;
   @vscode/tree-sitter-wasm's DOWN over a 51 KB unit, 62.6 KB), (B) origin claims the truth credits to the shipper
   (exp112's scorer 7 is the place: these are RIGHT names), and (C) shared generated code (wasm-bindgen glue,
   Go wasm_exec, Angular compiled templates). The next generic lever for A is index-side: record, per unit row,
   whether its FILE is a vendored copy (a path naming another package: `_chunks/libs/<pkg>`, `compiled/<pkg>/`,
   `chunks/<pkg>-<hash>`), so a run can refuse a seed that stands only on such rows. For C, a generated-glue
   stoplist learned from the index (prints shared across unrelated wasm packages) as exp108 suggested.

## Neutrality

Warm neutrality, branch head `2333caed` (all three keys, default off) vs main `f568e5bc`, 2.1.215 -> 2.1.216,
`--priors /work/exp050-cold --cache /work/neutrality-cache`: **NEUTRAL, 0 files / 0 lines, cache +0 / +0, exit
0 / 0.** (Library matching is off by default; a default flip of these keys would need report-mode neutrality as
exp109 did.)

Labels -> commits: `exp114-{cp,cp8,st,both}-*` and `test-exp114-cp-*` at `c82b0cd5`; `*-st2-*`, `*-both2-*` at
`ce110cb7`; `*-cpf-*`, `*-allf-*` at `2333caed`. Baselines: `exp109-final-k2-structural`,
`exp109-final-k2-apps-structural`, `test-exp109-final-k2-structural`, `test-exp109-final-wr-structural`. A
`test-exp114-st-*` run was stopped (st superseded by st2) and its partial label deleted.

## Reproduce

```bash
K2=/work/exp108-data/idx/ix/k2-l12m4; WR=/work/exp103-data/ix/step4-wr-v3
S="--pipeline-set library.discovery.shadow_ties=true"
CP="--pipeline-set library.discovery.context_presence=1.5 --pipeline-set library.discovery.context_presence_slack=16384"
npm run eval -- library exp114-st2-structural --index $K2 --rule structural $S
npm run eval -- library exp114-st2-apps-structural --index $K2 --rule structural --corpus apps --sets apps-esbuild $S
npm run eval -- library test-exp114-st2-k2-structural --index $K2 --rule structural --corpus test --sets test-bun,test-esbuild $S
npm run eval -- library test-exp114-st2-wr-structural --index $WR --rule structural --corpus test --sets test-webpack,test-rollup $S
# labs (dev corpora only), venv python = /work/exp095-library-fingerprints/scale/.venv/bin/python
$PY -I experiments/114-dev-precision/wrongmech.py exp109-final-k2-structural rows.jsonl     # section 1
$PY -I experiments/114-dev-precision/pkgpresence.py exp109-final-k2-structural
$PY -I experiments/114-dev-precision/seedlab.py exp109-final-k2-structural seeds.jsonl     # then simulate.py
$PY -I experiments/114-dev-precision/handread.py exp109-final-k2-structural exp114-st2-structural --n 15
```
