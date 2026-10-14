# exp109: the night's findings as the shipped default, measured end to end

## STATUS (2026-10-14)

DONE on branch `exp109-default-config` (off main `5fe24646`, main merged in through `829da98f`: exp107 scorer 6 +
the webpack module-table fix, exp108 k2/k5 indexes + `eval library --sets`). Gate: ALL 13 STAGES PASSED (at
`4435e68a` and again at the merged `ba5135c1`). Every number below is scorer `exp096-score/6`, structural rule.

- **Config defaults flipped** to exp106's recommendation (red/green: `library_config_test.rs`
  `the_default_library_tunables_are_the_measured_recommendation`). Library matching stays `off` by default.
- **Default behaviour is byte-neutral.** Warm neutrality, 2.1.215 -> 2.1.216, `--priors /work/exp050-cold --cache
/work/neutrality-cache`: `4435e68a` vs main `5fe24646` NEUTRAL 0 files / 0 lines, cache +0 / +0; merged `ba5135c1`
  vs main `829da98f` NEUTRAL 0 / 0, +0 / +0. Report mode (`--pipeline-set library.matching=report --pipeline-set
library.index=/work/exp103-data/ix/c2-l12m4-v3`) vs `5fe24646`: NEUTRAL 0 / 0, +0 / +0, and `diff -rq` of the two
  trees lists exactly one file, `.humanify/library-report.json`.
- **Default index: `k2-l12m4`** (exp108, Bun/esbuild) with **`step4-wr`** for webpack/rollup bundles, NOT the planned
  `c2-all`. The c2-all build (c2's package set x five slices) was started and STOPPED at ~43% (see "The index"
  below); `k2-all` (k2 x five slices) is the next build and reuses its store.
- **`literal_margin = 0` NOT shipped** (agreement rule only; inert under the structural default). Its test-webpack app
  claims are gone (0 under scorer 6 + exp107), and it raises F0.5 on every set, but it costs precision on the dev
  `apps` corpus (rollup 97.7 -> 95.3, webpack 98.8 -> 96.9).

## Totals first: the shipped default vs the night's baselines (P / R / F0.5 / F1; app claimed 0 everywhere)

| set                     | step4f (start of night) | exp101 c2, no guards    | **shipped: config + k2 / step4-wr** |
| ----------------------- | ----------------------- | ----------------------- | ----------------------------------- |
| held-out 67 (dev)       | 96.57/31.43/68.27/47.42 | 98.69/54.23/84.79/70.00 | **99.09/66.33/90.18/79.47**         |
| task-master-ai (dev)    | 98.02/40.61/76.41/57.42 | 98.38/52.55/83.77/68.51 | **95.69/65.36/87.57/77.67**         |
| wrangler (dev)          | 95.05/14.87/45.73/25.72 | 95.54/16.59/48.95/28.27 | **95.99/18.52/52.26/31.04**         |
| Claude Code (dev)       | 100/70.64/92.33/82.80   | 100/76.65/94.26/86.78   | **100/81.90/95.77/90.05**           |
| apps-esbuild (dev, 107) | 74.99/13.34/38.97/22.65 | 98.79/84.63/95.59/91.16 | **98.53/89.70/96.63/93.91**         |
| apps-webpack (dev, 107) | 97.15/47.71/80.47/63.99 | (no slice)              | 97.34/46.65/79.96/63.08 (step4-wr)  |
| apps-rollup (dev, 107)  | 89.26/35.21/68.29/50.49 | (no slice)              | 96.29/35.47/71.70/51.84 (step4-wr)  |
| dev-webpack (bundlers)  | 99.53/59.24/87.61/74.27 | (no slice)              | 99.59/59.24/87.65/74.28 (step4-wr)  |
| dev-rollup (bundlers)   | 99.57/71.13/92.20/82.98 | (no slice)              | 99.84/71.14/92.39/83.08 (step4-wr)  |
| TEST test-bun           | 52.72/14.06/34.01/22.20 | 75.38/44.18/66.05/55.71 | **80.58/55.99/74.07/66.07**         |
| TEST test-esbuild       | 51.39/14.28/33.82/22.35 | 77.45/46.50/68.35/58.11 | **83.61/66.16/79.42/73.87**         |
| TEST test-webpack       | 0/0/0/0 (no slice)      | 0/0/0/0 (no slice)      | 85.84/34.19/65.93/48.91 (step4-wr)  |
| TEST test-rollup        | 9.88/0.07/0.34/0.14     | 56.02/0.55/2.64/1.09    | 76.89/28.92/57.74/42.03 (step4-wr)  |

Baselines: `step4f-structural`, `test-step4f-structural` (main e9b8c0f0, re-scored); for the webpack/rollup rows the
start-of-night column is step4-wr with the OLD defaults (`exp107-cont-structural` for apps, `exp105-dev-structural`,
`test-exp105-structural` for test: 66.07 / 57.41 F0.5), since step4 has no such slice. The apps-esbuild baseline in
the c2 column is `exp109-apps-c2-none-structural`. The `apps` corpus is exp107's builds of TRAINING apps, the very
apps whose closures chose c2's and k2's packages, so its recall is in-sample. No set passes the 99.8% precision bar
except Claude Code.

## The config (what was flipped, and each key judged)

`[library.lookup] agreement = false`, `literal_margin_by_family = true`; `[library.discovery] evidence_conflict = true`,
`literal_conflict = true`, `contested_cover = 0.05`, `specific_rivals = true`, `banner_names = "known"`,
`derived_families = true`, `payload_min_bytes = 4096`; `helper_families` stays 0. The values live in
`LookupRules::recommended()` / `DiscoveryOptions::recommended()` (humanify-core) and the key registry reads them;
`Default` stays the engine's all-off baseline the unit tests compare each rule against (flipping `Default` itself
broke nine core tests whose "off" leg is `..Default::default()`). `humanify.example.toml` and
`docs/library-matching-config.md` carry the new values and how to set the old ones back.

Config effect at a FIXED index (old -> new defaults, F0.5):

| index    | set                        | old           | new           | note                                               |
| -------- | -------------------------- | ------------- | ------------- | -------------------------------------------------- |
| c2       | held-out / task-master     | 84.79 / 83.77 | 85.03 / 84.07 | exp106's numbers, reproduced                       |
| c2       | test-bun / test-esbuild    | 66.05 / 68.35 | 66.55 / 68.43 | precision 75.38 -> 76.08, 77.45 -> 77.55           |
| c2       | apps-esbuild               | 95.59         | 96.05         | P 98.79 -> 99.05, R 84.63 -> 85.69                 |
| step4-wr | dev-webpack / dev-rollup   | 87.61 / 92.20 | 87.65 / 92.39 |                                                    |
| step4-wr | test-webpack / test-rollup | 66.07 / 57.41 | 65.93 / 57.74 |                                                    |
| step4-wr | apps-webpack / apps-rollup | 80.47 / 68.29 | 79.96 / 71.70 | apps-rollup P 89.26 -> 96.29; apps-webpack R -1.06 |

With NO `--pipeline-set`, this branch's binary on step4-wr reproduces exp106's `test-exp106-wr-rec-structural`
(exp106's keys passed by hand) to the digit on all four test sets (label `test-exp109-wr-new-structural`, not
committed): the defaults ARE the recommendation.

- **`payload_min_bytes = 4096` judged: ON.** On k2 it is the difference between held-out precision 82.9 (exp108, one
  3.9 MB wasm-bindgen glue claim) and 99.1. Its cost is real and is on webpack: on one apps-webpack bundle
  (i5sing) it removes 1.1 MB of RIGHT claims, highlight.js language tables, moment locales (the 61,877 B loss
  repeated on five apps), psl's list, lodash, and it removes some `seed` claims as well as propagated ones. That is
  the apps-webpack -1.06 recall. Follow-up: the guard should accept a payload unit the claimed family's own
  index evidence holds in the bundle's slice (step4-wr's webpack slice is the thin as-of policy).
- **`literal_margin = 0` judged: NOT shipped.** exp107 branch `efd07bb2`, step4-wr, agreement + recommended keys,
  margin 10 -> 0, F0.5: test-bun 32.17 -> 33.73, test-esbuild 33.16 -> 35.11, test-webpack 59.21 -> 63.34,
  test-rollup 46.53 -> 52.41; apps esbuild 36.43 -> 37.51, rollup 54.19 -> 57.89, webpack 65.70 -> 69.77; app
  claimed 0 on all of them (exp107 cured the 3,118 B). But precision falls on every dev `apps` set (esbuild 82.27 ->
  81.43, rollup 97.71 -> 95.29, webpack 98.84 -> 96.90) and on test-webpack (97.89 -> 97.61). Under a 99.8% bar
  that is the wrong trade, and the key is inert under the structural default anyway.

## The index

**Shipped recommendation: `k2-l12m4` + `step4-wr`** (docs/design/library-index.md §10). k2 vs c2, both with the
shipped config (c2 dev = exp106's `exp106-c2-rec-structural2`): held-out F0.5 85.03 -> 90.18 (P 99.25 -> 99.09), test-bun 66.55 -> 74.07 (P 76.08 -> 80.58),
test-esbuild 68.43 -> 79.42 (P 77.55 -> 83.61), Claude Code 94.26 -> 95.77, apps-esbuild 96.05 -> 96.63. The one
regression is task-master precision 99.13 -> 95.69 (145 KB wrong-family + 79 KB foreign, top: `@vscode/tree-sitter-wasm`
over `web-tree-sitter`, `npm-check-updates` over `zod`, `@chromatic-com/storybook` over `@anthropic-ai/claude-agent-sdk`:
k2's newly indexed packages that ship PRE-BUNDLED copies of their dependencies, sponges exp097's filter cannot see).
F0.5 still rises there, 84.07 -> 87.57.

**Why not c2-all.** The plan was c2's package set across all five slices. Built with main's builder in a union store
(exp101's three slices + exp105's webpack/rollup, hard-linked) and a seeded work dir, the plan was 99,413 shards to
build (every literals shard rebuilds: its key holds the whole recipe SET; 29,011 webpack + 29,011 rollup; 2,579 x 3
known-failing retries) and 102,511 reused. On a box at load ~60, six chunk processes of ~230 versions took ~10 min each, a
projected 6.5 h; first as six long-lived parts (two reached 17-18 GB in 35 min: exp108's per-version leak), then in
exp108's short-lived chunks. When exp108/exp110 showed k2 beating c2 by 5-11 F0.5 with precision held, c2-all became
a build of the index that loses, holding the campaign lock for 5 more hours; it was stopped at 42/149 chunks. Nothing
is lost: `/work/exp109-data/store` holds ~12.6k new webpack/rollup shards and ~15.7k five-recipe literals shards for
c2 versions, and k2's set contains c2's roots, so k2-all reuses all of it. With exp111's builder (mimalloc, flat
memory) the k2-all build can run as long-lived parts.

## Reproduce

```bash
D=/work/exp109-data
# union store (hard links; shards are written by rename, never through a link)
for s in bun-bun-cjs-min bun-node-cjs-min esbuild-node-cjs-min literals; do cp -al /work/exp101-data/store/shards/$s $D/store/shards/; done
for s in webpack-node-cjs-min rollup-node-cjs-min; do cp -al /work/exp105-data/store/shards/$s $D/store/shards/; done
ln -s /work/exp101-data/registry $D/registry
grep '^index ' $D/logs/plan-c2-all.txt | awk '{print $2}' > $D/versions-c2.txt   # from a --dry-run plan
xargs -a $D/versions-c2.txt -P 12 -n1 experiments/109-default-config/scripts/seed-work.sh   # installs + builds
experiments/109-default-config/scripts/build-c2-all.sh $D <humanify-index>                 # under the build lock
# scoring (shipped defaults: no --pipeline-set)
K2=/work/exp108-data/idx/ix/k2-l12m4; WR=/work/exp103-data/ix/step4-wr-v3
npm run eval -- library exp109-final-k2-structural --index $K2 --rule structural
npm run eval -- library test-exp109-final-k2-structural --index $K2 --rule structural --corpus test --sets test-bun,test-esbuild
npm run eval -- library exp109-final-k2-apps-structural --index $K2 --rule structural --corpus apps --sets apps-esbuild
npm run eval -- library exp109-final-wr-bundlers-structural --index $WR --rule structural --corpus bundlers
npm run eval -- library test-exp109-final-wr-structural --index $WR --rule structural --corpus test --sets test-webpack,test-rollup
npm run eval -- library exp109-final-wr-apps-structural --index $WR --rule structural --corpus apps --sets apps-webpack,apps-rollup
# the old defaults, for a baseline at any index: --pipeline-set each key in docs/library-matching-config.md §2
```

Labels -> commits: `*-final-*` ran at `ba5135c1`; `test-exp109-c2-rec-structural`, `exp109-apps-c2-{rec,none}-structural`
at `78d8f24d` (crates identical to `4435e68a`; the later exp107 crate change touches webpack grouping only, and these
are Bun/esbuild sets). The literal-margin labels (`test-exp109-e7-*`, `exp109-e7-apps-*`) ran in a detached exp107
worktree at `efd07bb2` and are not committed (numbers above). All scored with scorer 6.
