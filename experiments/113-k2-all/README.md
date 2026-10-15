# exp113: k2-all (k2 x five recipe slices) and a precise payload guard

## STATUS (2026-10-15)

DONE on branch `exp113-k2-all` (off main `f568e5bc`; main merged in through `126dd3a9`: exp112 scorer 7 + `pop`, exp114
keys, exp116 `test2`). Every number below is scorer `exp096-score/7`, structural rule, the shipped defaults (no
`--pipeline-set`) unless a label says otherwise. Gate: ALL 13 STAGES PASSED, bare, before every commit. Warm neutrality
(2.1.215 -> 2.1.216, `--priors /work/exp050-cold --cache /work/neutrality-cache`): `5b3db379` vs `f568e5bc` and the
merged `12d9d1be` vs main `126dd3a9` both NEUTRAL 0 files / 0 lines, exit 0 / 0, cache +0 / +0.

- **B. payload guard fixed** (`5b3db379`, crates): it reads a unit's OWN bytes, and a base64 payload whose decoded head
  names the claimed package is that package's evidence. Pareto: recall up on held-out, task-master, apps-webpack,
  test-webpack, test2-webpack; no wrong-family byte added on any set; held-out precision 99.09 unchanged.
- **A. `k2-all` built** (`/work/exp113-data/ix/k2-all`, /3, 6.4 GB, id `6d503431`): k2's packages in all five slices.
  Bun/esbuild: identical to `k2-l12m4` on every set. Webpack/rollup vs `step4-wr`: F0.5 +10 to +36 on test, test2 and
  apps. **Documented as the recommended default index** (docs/design/library-index.md §10), one index for every
  bundler, with its costs stated: precision -2.0 on test2-rollup and -1.3/-1.8 on the small self-built dev `bundlers`
  corpus (a fork, `@redocly/ajv`, claiming ajv), where dev-webpack F0.5 is the one number that does not rise (87.65 ->
  87.53).

## Totals first (P / R / F0.5; app claimed 0 on every set)

| set                 | main shipped (k2 / step4-wr) | + exp113 guard        | **+ k2-all index**        |
| ------------------- | ---------------------------- | --------------------- | ------------------------- |
| held-out 67 (dev)   | 99.09 / 66.33 / 90.18        | 99.09 / 66.67 / 90.31 | **99.09 / 66.67 / 90.31** |
| task-master-ai      | 95.69 / 65.36 / 87.57        | 95.70 / 65.45 / 87.60 | 95.70 / 65.45 / 87.60     |
| wrangler            | 95.99 / 18.52 / 52.26        | =                     | =                         |
| Claude Code         | 100 / 81.90 / 95.77          | =                     | =                         |
| apps-esbuild        | 98.53 / 89.70 / 96.63        | =                     | =                         |
| apps-webpack        | 97.34 / 46.65 / 79.96        | 97.40 / 47.70 / 80.60 | **97.93 / 72.06 / 91.37** |
| apps-rollup         | 96.29 / 35.47 / 71.70        | 96.29 / 35.47 / 71.70 | **97.89 / 76.45 / 92.69** |
| dev-webpack         | 99.59 / 59.24 / 87.65        | =                     | 97.80 / 61.64 / 87.53     |
| dev-rollup          | 99.84 / 71.14 / 92.39        | =                     | 98.52 / 75.61 / 92.90     |
| pop-bun / pop-esb.  | 87.98 / 87.92 (F0.5)         | =                     | =                         |
| TEST test-bun       | 95.44 / 66.31 / 87.73        | =                     | =                         |
| TEST test-esbuild   | 95.86 / 75.86 / 91.06        | 95.86 / 75.87 / 91.06 | =                         |
| TEST test-webpack   | 97.95 / 39.02 / 75.23        | 97.96 / 39.19 / 75.36 | **98.30 / 70.04 / 90.96** |
| TEST test-rollup    | 94.62 / 35.59 / 71.05        | =                     | **97.09 / 67.94 / 89.42** |
| TEST2 test2-bun     | 98.52 / 77.65 / 93.50        | =                     | =                         |
| TEST2 test2-esbuild | 99.41 / 74.17 / 93.07        | =                     | =                         |
| TEST2 test2-webpack | 98.29 / 31.76 / 69.27        | 98.35 / 32.93 / 70.38 | **98.72 / 68.92 / 90.86** |
| TEST2 test2-rollup  | 96.69 / 44.87 / 78.55        | =                     | 94.65 / 72.98 / 89.34     |

Main labels: `exp109-final-*` (dev/apps/bundlers/test, re-scored under scorer 7 by exp112) / `exp113-k2-base`,
`exp113-wr-apps-base`, `pop-exp113-main-k2`, `test2-main-{k2,wr}-structural`. Guard: `exp113-own-*`,
`test-exp113-own-*`, `pop-exp113-own-k2`, `test2-exp113-own-{k2,wr}`. k2-all: `exp113-k2all`, `exp113-k2all-apps`,
`exp113-k2all-bundlers`, `pop-exp113-k2all`, `test-exp113-k2all`, `test2-exp113-k2all` (all at merged `12d9d1be`).

## B. What the guard got wrong, and the fix

The payload floor (exp106) refuses a PROPAGATED unit of >= 4,096 bytes, >= 90% inside string literals, that the claimed
family has no index evidence of. exp109 saw it also remove seeds and cost apps-webpack 1.06 R. Diffing the claims of
guard-on vs guard-off runs (same commit) found ONE mechanism behind every right byte lost on webpack:

- **It read a unit's SPAN, not its own bytes.** A module wrapper (webpack's `55672(e){...}` module function, a Bun/esbuild
  lazy module `var I=b(()=>{...})`) is a container unit of 9-25 bytes of its own whose span holds its body. The body is a
  unit of its own (`LazyBody`) and was a SEED by its own print (highlight.js languages, moment, psl, lodash, entities,
  regenerate-unicode-properties). Read by span, the wrapper looked like a 50-157 KB string payload with no print (a
  container has none), so GLUE was refused, and the seed, left without support, was then dropped by the isolation veto.
  That is exp109's "removes seeds too". Fix: the guard reads the unit's own bytes (`graph.units[u].bytes`); a body is
  judged as itself.
- **A base64 payload can say whose it is.** On held-out the guard also refused right opaque payloads: `@sentry/node-core`
  embeds its worker script as base64 (`LyohIEBzZW50cnkvbm9kZS1jb3JlIDEwLjc2LjIg...` = `/*! @sentry/node-core 10.76.2`),
  claimed by MODULE/DOWN on heroku, lighthouse and task-master (106 KB). Fix: an opaque run (>= 256 bytes of the
  base64/base64url alphabet) whose decoded head (first 1 KB) names the claimed package, standing alone (not inside a
  longer name), is that package's own evidence. The wasm-bindgen blob (`AGFzbQ...` = `\0asm`) names nothing and stays
  refused.

Refuted on the way (measured, not shipped): replacing the 90%-string test with an OPAQUE-run test ("half the unit's
bytes in base64 runs"), i.e. "only an opaque blob is a payload, a structured table is not". It reproduced the dev gains
exactly, but on TEST it admitted 141.6 KB of new wrong-family on test-bun (F0.5 74.07 -> 73.90 under scorer 6, 87.73 -> 87.53 under scorer 7) and 21 KB on
test-rollup, with no recall: few-node structured payloads (a JSON string, a keyword list under the row floor, where the
index can neither hold nor contradict the unit) reached by a wrong family's propagation are exactly what the string
test catches. The string scanner's known flaw (a regex holding a quote can flip it: lodash's module read 98% "string")
stays; every right byte the old guard removed on dev was a container or a self-naming payload, and with the fix the
guard-on and guard-off runs name the same right bytes on every dev set.

Tests (red first, then green): `a_module_wrapper_is_judged_by_its_own_bytes`,
`an_opaque_payload_that_names_its_package_is_its_own`, `an_opaque_run_inside_code_is_a_payload` (the wasm-bindgen
shape, a regression guard) in `crates/humanify-core/src/libdetect/discovery_test.rs`.

Labels (committed): `exp113-k2-base` (main, reproduces exp109's numbers to the digit), `exp113-k2-p0` /
`exp113-wr-apps-p0` / `exp113-k2-apps-p0` (guard off), `exp113-own-*` and `test-exp113-own-*` (the fix; built from
`5fa54c78`, a `git stash create` of the working tree whose crates differ from `5b3db379` by rustfmt only),
`test-exp113-opq2-*` / `exp113-opq2-*` (the refuted opaque variant, `e12049c0`).

## A. k2-all

**Build.** A hard-linked union store (exp109's, which holds exp101's three slices incl. k2's shards, exp105's and
exp109's ~12.6k webpack/rollup shards and ~15.7k five-recipe literals shards, topped up from exp101/exp105) and a work
dir seeded by HARD LINKS (`scripts/seed-work.sh`: installs and every FINISHED build, i.e. one holding its `.recipe`
stamp; safe because the builder never writes through a seeded file: a rebuild `remove_dir_all`s its out dir and writes
its stamp fresh, entries are `write_fresh`). Seeding 62,739 of 68,304 versions took 26 min and no copy. The plan:
51,065 rollup + 51,047 webpack + 52,582 literals shards to build, 5,565 x 3 known-failing Bun/esbuild retries; 62,739 x
3 Bun/esbuild, ~17.2k webpack/rollup and 15,722 literals reused. Built with main's builder (`f568e5bc`, mimalloc) in
exp108's chunk processes (125 chunks of ~400 pins, 7 at a time x 4 jobs, cpus 34-63, under `/work/index-build.lock`):
shards 43,491 s (12.1 h; chunk peak RSS 4-9 GB, every chunk exit 0), merge 1,049 s at 34.2 GB peak. 6.4 GB (/3).

**Score.** Bun/esbuild sets are byte-for-byte the same numbers as `k2-l12m4` (same slices, same packages). Webpack and
rollup gain 10-36 F0.5 on every held-out-style set (test, test2, apps); recall roughly doubles. The costs: test2-rollup
precision 96.69 -> 94.65 (summary only, not diagnosed), and the self-built dev `bundlers` corpus (27 bundles of step 4's
own packages) loses 1.3-1.8 precision points: 43 KB of new wrong-family, almost all `@redocly/ajv` (a fork carrying
ajv's code, now indexed in webpack/rollup) claiming ajv's modules by SEED. That is the fork/sibling family class (the
metadata families do not join this pair), not an index flaw; dev-webpack F0.5 87.65 -> 87.53 is the one number that does
not rise. Judged: k2-all is the recommended default (docs §10), one index for all five bundler slices; the fork pair is
the follow-up.

## Reproduce

```bash
D=/work/exp113-data; S=experiments/113-k2-all/scripts
# union store, hard links (shards are written by rename, never through a link)
mkdir -p $D/store/shards && cp -al /work/exp109-data/store/shards/* $D/store/shards/
for s in bun-bun-cjs-min bun-node-cjs-min esbuild-node-cjs-min literals; do cp -aln /work/exp101-data/store/shards/$s/. $D/store/shards/$s/; done
ln -s /work/exp108-data/idx/registry $D/registry
<humanify-index> build --packages /work/exp101-data/roots-base.txt --pinned /work/exp108-data/pins-k2.txt --as-of 2026-10-11 \
  --registry $D/registry --store $D/store --work $D/work --policy year-all+minor <five --recipe flags, see build-k2-all.sh> --offline --dry-run > $D/logs/plan-k2-all.txt
grep '^index ' $D/logs/plan-k2-all.txt | awk '{print $2}' > $D/versions-k2-all.txt
xargs -a $D/versions-k2-all.txt -P 12 -I{} $S/seed-work.sh $D/work {} /work/exp109-data/work /work/exp108-data/idx/work \
  /work/exp101-data/work /work/exp095-index-v2-data/selABC/work /work/exp105-data/work
$S/build-k2-all.sh $D <humanify-index @ f568e5bc>
# the payload fix (shipped defaults; k2 for Bun/esbuild, step4-wr for webpack/rollup)
npm run eval -- library exp113-own-k2 --index /work/exp108-data/idx/ix/k2-l12m4 --rule structural
npm run eval -- library test-exp113-own-k2 --index /work/exp108-data/idx/ix/k2-l12m4 --rule structural --corpus test --sets test-bun,test-esbuild
npm run eval -- library exp113-own-wr-apps --index /work/exp103-data/ix/step4-wr-v3 --rule structural --corpus apps --sets apps-webpack,apps-rollup
# k2-all, every corpus (test / test2: summaries only)
IX=/work/exp113-data/ix/k2-all
npm run eval -- library exp113-k2all --index $IX --rule structural
for c in apps bundlers; do npm run eval -- library exp113-k2all-$c --index $IX --rule structural --corpus $c; done
for c in pop test test2; do npm run eval -- library $c-exp113-k2all --index $IX --rule structural --corpus $c; done
```
