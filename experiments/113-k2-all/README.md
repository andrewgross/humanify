# exp113: k2-all (k2 x five recipe slices) and a precise payload guard

## STATUS (2026-10-14)

IN PROGRESS on branch `exp113-k2-all` (off main `f568e5bc`). Scorer `exp096-score/6`, structural rule, the shipped
defaults (no `--pipeline-set`) unless a label says otherwise.

- **B. payload guard fixed: DONE** (commit `5b3db379`). Gate: ALL 13 STAGES PASSED (bare, before the commit). Warm
  neutrality `5b3db379` vs main `f568e5bc`, 2.1.215 -> 2.1.216, `--priors /work/exp050-cold --cache
/work/neutrality-cache`: NEUTRAL 0 files / 0 lines, exit 0 / 0, cache +0 / +0 (library matching is off by default).
- **A. k2-all build: RUNNING** under `/work/index-build.lock` (30 cores) to `/work/exp113-data/ix/k2-all`.

## Totals first: the payload fix (P / R / F0.5; app claimed 0 on every set)

| set (index)              | main `f568e5bc`       | **exp113 guard**          | guard off (`payload_min_bytes=0`) |
| ------------------------ | --------------------- | ------------------------- | --------------------------------- |
| held-out 67 (k2)         | 99.09 / 66.33 / 90.18 | **99.09 / 66.67 / 90.31** | 83.22 / 66.67 / 79.28             |
| task-master-ai (k2)      | 95.69 / 65.36 / 87.57 | **95.70 / 65.45 / 87.60** | 95.70 / 65.45 / 87.60             |
| wrangler (k2)            | 95.99 / 18.52 / 52.26 | 95.99 / 18.52 / 52.26     | =                                 |
| Claude Code (k2)         | 100 / 81.90 / 95.77   | 100 / 81.90 / 95.77       | =                                 |
| apps-esbuild (k2)        | 98.53 / 89.70 / 96.63 | 98.53 / 89.70 / 96.63     | =                                 |
| apps-webpack (step4-wr)  | 97.34 / 46.65 / 79.96 | **97.40 / 47.70 / 80.60** | 97.40 / 47.70 / 80.60             |
| apps-rollup (step4-wr)   | 96.29 / 35.47 / 71.70 | 96.29 / 35.47 / 71.70     | 96.11 / 35.47 / 71.62             |
| dev-webpack / dev-rollup | 87.65 / 92.39 (F0.5)  | 87.65 / 92.39             | -                                 |
| TEST test-bun (k2)       | 80.58 / 55.99 / 74.07 | 80.58 / 55.99 / 74.07     | -                                 |
| TEST test-esbuild (k2)   | 83.61 / 66.16 / 79.42 | **83.61 / 66.18 / 79.43** | -                                 |
| TEST test-webpack (wr)   | 85.84 / 34.19 / 65.93 | **85.90 / 34.36 / 66.08** | -                                 |
| TEST test-rollup (wr)    | 76.89 / 28.92 / 57.74 | 76.89 / 28.92 / 57.74     | -                                 |

The fix takes every RIGHT byte that switching the guard off gains on dev (held-out +0.34 R, apps-webpack +1.05 R) and
none of the wrong ones (the 3.9 MB wasm-bindgen claim, 67 KB on apps-rollup). No set loses a byte; no wrong-family byte
is added anywhere, dev or test. Held-out precision stays 99.09.

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
exactly, but on TEST it admitted 141.6 KB of new wrong-family on test-bun (F0.5 74.07 -> 73.90) and 21 KB on
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

(pending)

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
```
