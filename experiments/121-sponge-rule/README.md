# exp121: a sponge rule — stop packages that inline their dependencies claiming that code

## STATUS (2026-10-16)

DONE on branch `exp121-sponge-rule` (off main `d2d08a05`, main `7daf1f0b` (exp120, scorer 9) merged in). NOT
merged. Four new `[library.discovery]` keys, all default OFF, written test-first (`discovery_test.rs`:
`copy_presence_*`; `library_config_test.rs`). The gate passed bare (ALL 13 STAGES) before every commit. Every
number is scorer 9, the structural rule, main's defaults plus the keys named.

- **The rule (`copy_presence`).** Take an index seed of package P. It is a COPY seed when another family Q holds
  at least `copy_presence` of the nodes P's prints cover there, and Q holds each of those prints in an index module
  at most 1/`copy_presence_factor` the size of P's module for the same print. Q is then the smaller context: the
  origin that P's dist inlined. Only prints the lookup itself reads count (at most `max_packages` owners within
  specificity).

  A package is judged per PACKAGE, not per family. It "shows" in the bundle only when its non-copy seeds hold at
  least `copy_presence_own` of its seed bytes. A package that does not show loses its copy seeds. With
  `copy_presence_origin`, each lost seed goes to that origin (the owner co-holding the most, in the smallest
  module). Without it, nobody gets it (it is fenced with `presence_fence`).

  The measured setting is `v2`: 0.1 / factor 4 / origin on / own 0.1.
- **On k2-all-dl25k (the bigger index):**
  - It removes the sponge class that exp118 found: held-out F0.5 goes 90.42 -> **95.26** (P 99.24 -> **99.56**).
  - pop, apps-rollup and apps-webpack beat k2-all. Every test2 set does too, or ties (bun +0.76, esbuild +0.57,
    rollup -0.00, webpack +0.80).
  - **But the frozen TEST split (exp100) does not recover.** test-bun is 88.96 -> 84.11 and test-esbuild is
    91.49 -> 87.64, the same as dl25k without the rule. The bar ("beat k2-all on held-out, pop, test AND test2")
    is **NOT met**, so k2-all-dl25k is still not shippable.
- **On k2-all (today's default index), v2 is clean on all 19 sets.** No set's F0.5 or precision goes down, and
  app claimed is 0 everywhere. Gains: test-bun +0.43, test-esbuild +0.45, pop +0.23 / +0.27, apps-esbuild +0.20,
  test2-rollup +0.12, test2-webpack +0.06. Everything else is equal. **Recommended: flip v2 into
  `recommended()`** (a separate lane: report-mode neutrality with the keys on).
- **The existing candidates, re-measured on k2-all-dl25k (lab):**
  - exp110's index-time origin dedup reaches at most ~24 KB of held-out's 420 KB wrong (and ~7 KB of
    task-master's 341 KB), even when it assumes every dependency relation holds.
  - exp115's vendored-path marks reach 2.6 KB held-out and 0 task-master.
  - The presence keys miss the dominant sponge for a measurable reason (section 1).

## Totals (P / R / **F0.5**, dF0.5 vs k2-all; wrong-family KB)

before = `*exp119-k2all-st*` (k2-all, main defaults), dl25k = `*exp118-dl25k*` (re-scored under scorer 9), v2 =
`*exp121-v2*` (k2-all-dl25k + v2), k2+v2 = `*exp121-k2-v2*` (k2-all + v2).

| set | k2-all (before) | k2-all-dl25k | **dl25k + v2** | **k2-all + v2** |
|---|---|---|---|---|
| held-out | 99.24 / 66.69 / **90.42** (155) | 98.30 / 80.16 / 94.05 (+3.63; 420) | 99.56 / 81.23 / **95.26** (+4.84; 109) | 99.25 / 66.70 / 90.42 (+0.01; 154) |
| task-master (esbuild-app) | 96.25 / 65.50 / **87.99** (126) | 89.71 / 60.54 / 81.83 (-6.16; 341) | 98.53 / 66.50 / **89.87** (+1.89; 49) | = |
| wrangler | 95.99 / 18.53 / **52.28** | 99.37 / 20.92 / 56.79 (+4.51) | 99.37 / 20.92 / 56.79 (+4.51) | = |
| Claude Code | 100 / 81.90 / **95.77** | 100 / 82.46 / 95.92 (+0.15) | 100 / 82.46 / 95.92 (+0.15) | = |
| apps-esbuild | 98.95 / 89.91 / **97.00** (245) | 98.94 / 88.66 / 96.70 (-0.30) | 99.12 / 88.81 / 96.87 (-0.13; 203) | 99.16 / 90.09 / **97.20** (+0.20; 197) |
| apps-rollup | 98.92 / 77.40 / **93.71** | 98.98 / 80.87 / 94.74 (+1.03) | 99.00 / 80.90 / 94.76 (+1.05) | 93.72 (+0.01) |
| apps-webpack | 99.39 / 73.33 / **92.79** | 99.33 / 75.01 / 93.28 (+0.49) | 99.33 / 75.04 / 93.30 (+0.50) | = |
| dev-rollup | 99.87 / 77.53 / **94.43** | 94.37 (-0.06) | 94.37 (-0.06) | = |
| dev-webpack | 99.64 / 62.89 / **89.22** | 89.11 (-0.11) | 89.11 (-0.11) | = |
| pop-bun | 95.13 / 77.17 / **90.90** (3,998) | 95.89 / 76.92 / 91.38 (+0.48) | 96.40 / 77.35 / **91.87** (+0.97; 2,927) | 95.37 / 77.38 / 91.13 (+0.23) |
| pop-esbuild | 93.65 / 79.66 / **90.47** (5,484) | 94.85 / 79.58 / 91.34 (+0.87) | 95.32 / 79.98 / **91.80** (+1.33; 3,984) | 93.93 / 79.89 / 90.74 (+0.27) |
| TEST bun | 97.23 / 66.39 / **88.96** (1,236) | 91.46 / 63.52 / 84.06 (-4.90) | 91.51 / 63.56 / 84.11 (**-4.85**; 3,849) | 97.79 / 66.54 / **89.39** (+0.43; 981) |
| TEST esbuild | 96.42 / 75.96 / **91.49** (1,947) | 92.23 / 72.79 / 87.55 (-3.94) | 92.32 / 72.88 / 87.64 (**-3.84**; 4,180) | 96.96 / 76.17 / **91.94** (+0.45; 1,646) |
| TEST rollup | 97.89 / 68.39 / **90.12** | 96.40 / 70.58 / 89.83 (-0.29) | 96.41 / 70.61 / 89.85 (-0.27) | 90.13 (+0.02) |
| TEST webpack | 99.19 / 70.51 / **91.73** | 97.95 / 74.02 / 92.00 (+0.27) | 97.95 / 74.02 / 92.00 (+0.27) | 91.78 (+0.05) |
| TEST2 bun | 98.71 / 77.70 / **93.64** (424) | 96.15 / 78.25 / 91.94 (-1.70) | 98.72 / 80.34 / **94.40** (+0.76; 433) | = |
| TEST2 esbuild | 99.43 / 74.23 / **93.10** (185) | 96.86 / 73.81 / 91.17 (-1.94) | 99.52 / 75.84 / **93.67** (+0.57; 157) | = |
| TEST2 rollup | 96.18 / 74.74 / **90.96** | 96.10 / 74.84 / 90.94 (-0.02) | 96.11 / 74.90 / 90.96 (-0.00) | 96.28 / 74.90 / 91.08 (+0.12) |
| TEST2 webpack | 99.14 / 69.22 / **91.25** | 99.25 / 71.30 / 92.04 (+0.79) | 99.27 / 71.32 / 92.05 (+0.80) | 99.20 / 69.26 / 91.31 (+0.06) |

"=" means identical to k2-all. App claimed is 0 on every label. test and test2 are summaries only; neither was
diagnosed.

The variants on the way (k2-all-dl25k, labels `*exp121-<tag>*`):

- `cp10f4`: 0.1 / factor 4, family-keyed. It missed @mastra/server, which a scope sibling vouched for.
- `cpp10f4`: per package, fenced. Held-out 94.92, task-master 87.48, test2 bun 93.78 / esbuild 93.03 / rollup
  90.92 / webpack 92.20, test-bun 84.09.
- `cpo10f4`: plus the origin. Held-out 95.19, task-master 89.87, test2 94.40 / 93.67 / 90.97 / 92.27.
- `cpow`: plus own 0.1. Held-out 95.26.

`cpow` read every print P holds, including prints too common for the lookup. On k2-all that made jiti's right
origin claims on pop-esbuild into "copies" of the Babel helpers jiti inlines, and pop-esbuild dropped -3.46 /
test-esbuild -5.30 (`*exp121-k2-cpow*`). v2 counts only the prints the lookup reads, which fixed it (red test
`copy_presence_reads_only_the_prints_the_lookup_reads`). On dl25k, v2 is cpow, except test2-webpack 92.27 -> 92.05
and test2-rollup 90.97 -> 90.96.

## 1. Why the shipped keys miss the sponges (dev, k2-all-dl25k; `scripts/copylab.py`, `wrongseeds.py`)

The new wrong bytes in held-out and task-master are 96% seeds. **One unit is 293 KB of each set's wrong.** It is
the 147 KB module in which `@modelcontextprotocol/server`'s dist bundles ajv (truth: shipper MCP, origin ajv). In
k2-all, ajv claimed it right (origin). In dl25k, @mastra/server claims it: mastra's dist inlines the same ajv build
in a 164 KB module and covers 34,094 of its 43,679 nodes.

- **context_presence** compares P's module with the BUNDLE's module group. Here both are copies of about the same
  size (164 KB vs 148 KB), so the ratio is 1.1, far below 1.5 x group + 16 KB.
- **shadow_ties** needs a tie. ajv covers only ~24% of the unit, because the dl25k packages that also hold these
  prints push some of them over `max_packages`. So mastra is accepted alone.
- **presence_fence** only fences what the others drop.
- **exp115's marks** need a file path that names another package. Mastra's copy is `dist/index.cjs` / chunks.
  The ceiling on the dl25k labels (`ceiling.py` on marks re-derived for this index): held-out 2.6 KB of 420 KB,
  task-master 0 of 341 KB, pop 42 / 75 KB, apps-esbuild 35 KB.
- **exp110's dedup** leaves alone any print with more than 3 owners. Mastra's prints are also held by
  `@datadog/datadog-ci`, `@anthropic-ai/claude-agent-sdk`, `@cursor/sdk`, `sentry` and others: sponges of the same
  ajv. Its upper bound on the dl25k seeds (`exp110sim.py`, every dependency relation ASSUMED true) reaches at most
  24 KB of held-out's wrong and 7 KB of task-master's, against 1.8 MB / 135 KB of good seeds it would put at risk.
  The real rule needs dependency metadata, which does not exist for the 13.8k new packages: the work dirs were
  pruned, and store/declared/ predates them.

**What does see it.** ajv holds 7,818 of mastra's 34,094 covered nodes (23%) in modules of 250 B to 2 KB. Those
are ajv's own files, more than 4x smaller than mastra's 164 KB module. That is idea 4 of the brief, and the
signal `copy_presence` reads.

## 2. Lab (seed-level simulation, `scripts/copysim.py`, `copywho.py`, `reassign.py`, `copyfrac.py`)

At the package level (P's every seed is a copy seed), factor 4, share 0.1, on the dl25k labels, the rule removes:

| set | wrong | good |
|---|---|---|
| held-out | 295 KB | 0 |
| task-master | 293 KB | 0 |
| pop-bun | 344 KB | 0 |
| pop-esbuild | 315 KB | 8.6 KB |
| apps (all three) | 15 KB | 0 |

Removing each copy seed on its own (seed level) costs 67-891 KB of good per set at any setting, so the rule must
be package-level. That is the same lesson as exp115.

Who it removes: @mastra/server <- ajv, meow <- yargs-parser, @rollup/wasm-node <- magic-string, @rsdoctor/utils <-
finalhandler, ora <- yoctocolors, @likec4/core <- remeda, @livekit/components-react <- clsx, webpackbar <- wrap-ansi.

**Prefer the origin.** For the seeds dropped, the best co-holder Q is the truth or its origin for about 98% of
the bytes (`reassign.py`). That is why `copy_presence_origin` exists. It turns the fenced 293 KB back into right
ajv claims: task-master 87.48 -> 89.87, held-out recall +1.0.

**Own share.** With `copy_presence_own` = 0.1, a package's own seeds must carry 10% of its seed bytes before they
vouch for its copies. Lab: +20 KB wrong removed on apps-esbuild and +19 / +62 KB on pop, with 0 good lost.

## 3. Hand-reads (dev; `../114-dev-precision/handread.py` dl25k -> cpo10f4, 8 groups each from dev and pop)

| # | bundle | was | now | truth | verdict |
|---|---|---|---|---|---|
| 1 | held-out vite | @likec4/core (seed) | remeda | rolldown chunk, origin remeda | fixed: remeda's `purry` |
| 2 | held-out dotenvx | ora (seed) | unclaimed | yoctocolors base.js | wrong removed: yoctocolors' `format(0,0)` table |
| 3 | held-out swc_cli | @japa/core (seed) | unclaimed | time-span index.js | wrong removed |
| 4 | held-out vite | @rollup/wasm-node (seed, selfOther) | magic-string (selfOrigin) | `<self>`, origin magic-string | fixed: the MagicString class |
| 5 | task-master (esbuild) | @mastra/server (seed) | ajv (right) | ajv dist/core.js | fixed: ajv's codegen (`_Code`, `Name`) |
| 6 | held-out task-master | @mastra/server (seed) | ajv (origin) | @modelcontextprotocol/client, origin ajv | fixed |
| 7 | held-out vite | @rollup/wasm-node (seed) | unplugin | `<self>`, no origin | neutral (self both ways) |
| 8 | held-out vite | @rollup/wasm-node (down) | unclaimed | `<self>`, origin sourcemap-codec | wrong removed |
| 9 | pop-esbuild eslint-config-inspector | @mastra/server (seed) | ajv (right) | ajv dist/core.js | fixed |
| 10 | pop-esbuild romie | @rsdoctor/utils (down, 2 B) | obug (origin) | vite chunk, origin obug | fixed |
| 11 | pop-bun hackdesk | @uiw/react-codemirror (seed) | crelt (right) | crelt index.js | fixed |
| 12 | pop-esbuild ng-packagr | @likec4/core (seed) | remeda (origin) | rolldown chunk, origin remeda | fixed |
| 13 | pop-esbuild romie | @rslib/core (seed) | resolve-pkg-maps (wrong) | tsx chunk, no origin | wrong -> wrong. tsx's chunk inlines resolve-pkg-maps-style code; arguably a truth gap |
| 14 | pop-bun opennextjs | webpackbar (seed) | wrap-ansi (right) | wrap-ansi index.js | fixed |
| 15 | pop-bun eslint-config-inspector | @mastra/server (seed) | ajv (origin) | @modelcontextprotocol/server, origin ajv | fixed |
| 16 | pop-esbuild romie | @likec4/core (seed) | remeda (origin) | as 12 | fixed |

Of the 16: 14 were fixes or removed wrong claims, 1 was neutral, and 1 went from wrong to wrong. No right claim was
lost.

## 4. What is left: why the TEST split still loses on dl25k

test-bun keeps +2.6 MB of wrong under dl25k + v2, and test-esbuild +2.2 MB. The rule moves those by about 25 KB.
test2 (also Bun/esbuild) recovers fully, and so do pop (the dev analogue of the test population) and held-out. So
the mechanism behind test's loss is one that none of the dev corpora show, and this lane did not diagnose test.

exp112 found test's wrong bytes heavy-tailed: one formatter package in two apps made up 88-90% of them. A heavy
tail of that kind, from one or two new dl25k packages, fits these numbers. Neither origin evidence nor the
own-share signal reaches it (the copy seeds' origins would need a smaller-context co-holder).

Next step: a lane allowed to read test's aggregates (as exp112 was) can decompose the 2.6 MB by claimer. Or a fresh
dev corpus of the test population that reproduces the loss. Until then, download-rank growth stays unshipped.

## Neutrality

All four keys are default off, so the pipeline cannot move. Warm neutrality vs main `7daf1f0b`, pair 2.1.215 ->
2.1.216 (`--priors /work/exp050-cold --cache /work/neutrality-cache`, under `flock /work/neutrality.lock`, from a
clean detached worktree of the branch head): see the commit that adds this README's final status line.

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python; S=experiments/121-sponge-rule/scripts
$PY -I $S/copylab.py [--corpus pop|apps] <label> <out.jsonl>         # per index seed: covering prints + co-holders
$PY -I $S/copysim.py <copy.jsonl>...                                  # seed / package rule grid (F, share)
$PY -I $S/copywho.py 4 0.1 <copy.jsonl>...; $PY -I $S/reassign.py 4 0.1 <copy.jsonl>...; $PY -I $S/copyfrac.py 4 0.1 <copy.jsonl>...
$PY -I $S/exp110sim.py <copy.jsonl>...                                # exp110 upper bound
$PY -I $S/wrongseeds.py <seedlab.jsonl>                               # exp114 seedlab rows: where the truth stands
$PY -I $S/claims_c.py --corpus pop diff <base> <label>                # exp106 claims.py on any dev corpus
# exp115 ceiling: marks.py on each slice of k2-all-dl25k, then ceiling.py (branch exp115-vendored-paths)
V2="--pipeline-set library.discovery.copy_presence=0.1 --pipeline-set library.discovery.copy_presence_factor=4 \
    --pipeline-set library.discovery.copy_presence_origin=true --pipeline-set library.discovery.copy_presence_own=0.1"
npm run eval -- library exp121-v2 --index /work/exp118-data/idx/ix/k2-all-dl25k --rule structural $V2  # + --corpus apps|bundlers|pop|test|test2
npm run eval -- library exp121-k2-v2 --index /work/exp113-data/ix/k2-all --rule structural $V2
python3 -I $S/table.py <before> <label>...                            # the tables above
```
