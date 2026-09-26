# 19 — The cutover (phase 6): the Rust binary is the only pipeline

**STATUS (2026-09-26): DONE on branch `rust/cutover`, pending the owner's
sign-off to merge.** `src/` (the TypeScript pipeline) is deleted; the Rust
binary `target/release/humanify` is the pipeline; everything the harness
needed from `src/` was moved into `experiments/lib/js/` or retired with a
reason below. Plan of record: `17-formatter-swap.md` §6. Preceded by M4
(`18-5b-eval-result.md`: the self-sufficient binary holds novel/realLn exactly,
judged cold). Tag `m4` (cd3a349) is the last tree with the TS pipeline — git
history is the archive; no copies of deleted code were kept.

## 1. The proof

The eval is the judge; the TS-vs-Rust byte gates are retired with the TS.

| check                                                  | result                                                                                                                                                                                                                   |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `npm run check`, bare                                  | ALL 13 STAGES PASSED (list in §5)                                                                                                                                                                                        |
| KPI scorer unchanged — per-pair cards                  | §2: the three labels' 12 trees re-scored by the pre-cutover scorer (cd3a349) AND the post-cutover scorer: every card byte-identical to the committed original, both ways                                                 |
| KPI scorer unchanged — summary                         | §2: `summarize.ts` over copies of the three labels (re-scored cards in place): `summary.json` byte-identical to each original                                                                                            |
| cold smoke, new default (no `--bin`), `--pairs 85->86` | §3                                                                                                                                                                                                                       |
| the binary's e2e fixture stage                         | passes: 2 fixture pairs, fresh + prior, deterministic, boots with the input's surface — and surfaced finding #55                                                                                                         |
| the Rust half deleted nothing the pipeline runs        | the release binary at cd3a349 vs after the verb deletion, on 4 inputs (both e2e fixtures fresh + prior with `--dump-artifacts`, a wrapped bundle through `--split` twice, a Bun bundle): 120 output files byte-identical |

## 2. KPI-scorer byte identity

The KPI hash (`statementHash`) and everything the scorer reached in `src/`
moved to `experiments/lib/js/` (a `git mv` for the two whole files; the
others copied verbatim from their functions). Proof, on the committed labels
`ts-control-fec64e5-r2`, `rust-5b-c3b272f-a`, `rust-5b-c3b272f-b` (results in
the main checkout, read only; trees under `/tmp/eval-work/<label>/`):

- each pair's `analyze.ts` re-run with the exact arguments run.sh used, once
  with the scorer at cd3a349 (a frozen worktree, `/work/cutover-proof-base`)
  and once with this branch's scorer; outputs under `/work/cutover-proof/`
  (`base/`, `new/`), log `/work/cutover-proof.log`;
- **cards: 24/24 byte-identical to the committed originals** (12 pair cards ×
  2 scorers), and base = new on every one;
- **summaries: `summarize.ts` over copies of each label holding the re-scored
  cards writes a `summary.json` byte-identical to the original — 3/3 labels,
  both scorers.** Its printed table differs between the two runs only in the
  `wrote <path>` line (the two checkouts' paths);
- **leaderboard** over the three labels (novel 4,188 / realLn 416,377 on
  every row): new and base output byte-identical
  (`/work/cutover-proof/{new,base}/leaderboard.txt`).

## 3. Cold smoke on the new default

`npm run eval -- score cutover-smoke --pairs '85->86'` — no `--bin`, so the
harness built and scored `target/release/humanify` at the branch's first
cutover commit (8c8c685, clean tree). Cold (`cache +0`, every prompt live),
run alone, endpoint `http://192.168.1.234:8000/v1` (`openai/gpt-oss-20b`,
low, concurrency 32). Label `cutover-smoke` (untracked, as every label).

| check                 | result                                                                                                                                    |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| rebase of 2.1.85      | ran (no "rebase FAILED")                                                                                                                  |
| pipeline exit         | 0, errors []                                                                                                                              |
| **novel / realLn**    | **787 / 78,791 — exactly the 5b references' (TS, A, B)**                                                                                  |
| noise / reloc / mints | 1,495 / 654 / 14 — inside the bands 18-5b-eval-result.md judged by (TS 1,500 / 650 / 16)                                                  |
| boot gate             | OK, both halves (`--version` 2.1.86, live prompt `boot-ok`)                                                                               |
| cold self-hop         | 80 bundle lines — below the recorded 92–180 range, as the 5b runs were (TS 54, A 90, B 38); below is not a failure (18-5b-eval-result.md) |
| warm self-hop         | OK — byte-identical tree, 0 cache writes, exit 0                                                                                          |

## 4. Importer inventory — every use of `src/` outside `src/`, and its disposition

### Moved into the harness (`experiments/lib/js/`, Babel a devDependency)

| needed by                                                             | piece                                                                                                                               | now                                                                                   |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `034/statements.ts`, `037/diff-composition.ts` (the KPI hash)         | `statementHash` (`src/split/statement-hash.ts`)                                                                                     | `js/statement-hash.ts` (git mv; `STATEMENT_HASH_VERSION` dropped — no harness reader) |
| `lib/trees.ts` (`bundleStatements`)                                   | `findWrapperFunction` (`src/analysis/wrapper-detection.ts`)                                                                         | `js/wrapper.ts` (git mv; debug log line dropped)                                      |
| `lib/trees.ts`, `046/vendor-churn.ts`, `scripts/clone-census.ts`      | `parseFileAst` / `parseSourceAst`, `traverse`, cache clear (`src/babel-utils.ts`)                                                   | `js/babel.ts` (same parse options, same big-source cache clear)                       |
| `lib/trees.ts`, `lib/diff.ts`                                         | `listJsFilesRecursive` (`src/file-utils.ts`), `METADATA_DIR` (`src/split/layout.ts`), the ledger type (`src/split/stable-split.ts`) | `js/tree-layout.ts` (`SplitLedger`, the harness's read view)                          |
| `lib/diff.ts`, `055/real-ledger.ts` (the `eval diff` verb)            | `computeNormalDiff`, `tokenizeLine` (`src/rename/diff-reconcile.ts`)                                                                | `js/line-diff.ts` (verbatim ranges)                                                   |
| `046/vendor-churn.ts` (vendor KPI columns), `scripts/clone-census.ts` | `serializePathTokens` (`src/analysis/structural-hash.ts`)                                                                           | `js/structural-tokens.ts` (verbatim; per-AST binding memo kept)                       |
| the binary's webcrack adapter (runtime, webpack/browserify only)      | `webcrack()` (`src/plugins/webcrack.ts`)                                                                                            | absorbed into `scripts/webcrack-shim.ts`; `webcrack` stays the one runtime dependency |

### Retargeted to the binary

| instrument                                | before                                         | now                                                                                                                                                                                                                                                                                                                                                                                                     |
| ----------------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `034/run.sh` (`npm run eval -- score`)    | `npx tsx src/index.ts` unless `--bin`          | the binary always: `--bin` defaults to `target/release/humanify`, built and recorded by `pipeline-bin.ts`. Red-first test: `run-launch.test.ts` "WITHOUT --bin the harness builds and runs the repo's own binary — the TS mode is gone". Launch golden re-captured from the pre-cutover run.sh under `--bin` (the command lines `rust-5b-*` were scored by); the new run.sh reproduces it byte for byte |
| `lib/run-pipeline.ts` / `pipeline-bin.ts` | a config without `command` launched the TS     | refused (`pipelineCommandOf` throws; red-first in `pipeline-bin.test.ts`); `tsPipelineCommand` deleted                                                                                                                                                                                                                                                                                                  |
| `lib/neutrality.sh`                       | each leg `npx tsx <leg>/src/index.ts`          | each leg builds and runs its own commit's binary (`lib/build-bin.sh`); `--heap-mb` dropped                                                                                                                                                                                                                                                                                                              |
| `lib/gate.sh`, `lib/selfhop.sh`           | `npx tsx $REPO/src/index.ts`                   | the binary, built once per run (`lib/build-bin.sh`)                                                                                                                                                                                                                                                                                                                                                     |
| `scripts/eval.ts`                         | `score` without `--bin` = TS; `preflight` verb | `--bin` optional (default the repo's binary); the dispatcher refuses to add cards to a pre-cutover TS label; `preflight` verb, `--skip-preflight`, `--warm-self-hop` gone (the warm self-hop always runs)                                                                                                                                                                                               |
| `scripts/clone-census.ts`                 | scanned `src/` with the pipeline's serializer  | scans the living TS (scripts/, experiments/lib, the 034 harness, test/) with `js/structural-tokens.ts`; its first run's eight pre-existing 034 groups allowlisted with reasons                                                                                                                                                                                                                          |
| `scripts/rust-parity.ts` (`rust:parity`)  | selftest + committed TS fixture dumps          | selftest only; the differ compares Rust vs Rust dumps now                                                                                                                                                                                                                                                                                                                                               |

### Retired, with the reason

| what                                                                                                                                                                                                                                                                                                              | why                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `test/e2e/harness/`, the six `*.fptest.ts`, `test/e2e/snapshots/`, `test/e2e/functional.test.ts`, the mitt/nanoid/preact/zustand fixture configs                                                                                                                                                                  | the fingerprint harness validated the **TS matcher** (`fingerprint-index`, `function-graph`, `rename/plugin`) against npm packages; that matcher is deleted. `functional.test.ts` tested upstream nanoid's own source (from a harness clone), not humanify. **Follow-up:** a Rust verb exposing the matcher (two files in → pairs out) would let the ground-truth validation return against the binary; not built here |
| `experiments/lib/matcher-preflight.sh` (+ the eval's preflight step)                                                                                                                                                                                                                                              | same subject (the TS matcher); under `--bin` it already covered nothing the scored pipeline ran. Old labels' `preflight-status.json` is still READ by the summary (the byte-identity proof depends on that)                                                                                                                                                                                                            |
| `src/**/*.e2etest.ts` (one: `src/test/rename.e2etest.ts`), `npm run test:e2e`'s old meaning                                                                                                                                                                                                                       | tested the TS rename plugin with a mock provider; replaced by `scripts/e2e.ts` (the binary + a stub LLM + a boot check)                                                                                                                                                                                                                                                                                                |
| `experiments/lib/counterfactual.ts`, `verify-counterfactual.ts`, `size-binding-singleton-guard.ts`, `size-callee-set-symmetry.ts`, `size-private-name-match.ts`                                                                                                                                                   | ran the TS splitter/emitter/matcher in-process to size levers; the Rust stages are not callable from TS. A sizing now runs the binary (`--disable` ablations via `gate.sh`) or is written in Rust                                                                                                                                                                                                                      |
| `experiments/034-eval-harness/ceiling-identity-recovery.ts`, `ceiling-lever2.ts`                                                                                                                                                                                                                                  | one-off ceilings importing TS pipeline types/modules                                                                                                                                                                                                                                                                                                                                                                   |
| 190 more files under historical `experiments/NNN/` (appendix)                                                                                                                                                                                                                                                     | imported `src/` or launched `src/index.ts` (directly or through a file that did); dead code without it. Their READMEs/RESULTS/data stay; experiments/README.md says how to re-run one at tag `m4`                                                                                                                                                                                                                      |
| `experiments/prepare*.ts`, `metrics.ts`, `report.ts`, `types.ts`, `fixtures.json`, `proximity-merge.ts`, `npm run experiment:{prepare,run}`                                                                                                                                                                       | the pre-034 experiment framework: `experiment:run` pointed at a file that no longer existed, `prepare-humanified.ts` drove the TS pipeline; with it went the devDependencies `esbuild`, `terser`, `@swc/core`, `source-map`                                                                                                                                                                                            |
| `test/parity/` probes (every `*.ts`/`*.mjs`/`*.sh`/`*.py`/`*.js`) and the TS artifact dumps `test/parity/<fixture>/ts/`                                                                                                                                                                                           | produced the frozen specs by running the TS; the specs (JSON) stay as the Rust tests' frozen inputs. Kept from the dumps: `ts/text/{prior,fresh}.js` (twins_test.rs reads them). Also removed: `format-cases.json` (its inputs are embedded in `format-goldens.json`) and `library-carry-inputs.json` (probe input only)                                                                                               |
| `scripts/switch-census.ts` (+ test), `scripts/exec-census.ts`, `npm run census:{switches,exec}`                                                                                                                                                                                                                   | inventoried the TS pipeline's toggles and execution counts under `src/`. The Rust side's registries are `humanify_cli::kill_switches` and the env-read confinement clippy enforces                                                                                                                                                                                                                                     |
| `scripts/wp12-counts.sh`                                                                                                                                                                                                                                                                                          | the WP1.2 oxc-vs-Babel counts gate over TS oracle dumps                                                                                                                                                                                                                                                                                                                                                                |
| Rust test `the_ledger_applier_is_the_ts_template`                                                                                                                                                                                                                                                                 | compared `RENAME_LEDGER_APPLIER` with the TS source's literal; the Rust constant is now the spec (its content is still asserted by the ledger-dir test)                                                                                                                                                                                                                                                                |
| npm scripts `start`, `debug`, `build` (pkgroll), `deploy:*`, `e2e`, `e2e:humanify`, `test:fingerprint*`; `main`/`bin`; runtime deps `@babel/generator`, `@babel/plugin-transform-explicit-resource-management`, `babel-plugin-transform-beautifier`, `commander`, `dotenv`, `globals`, `openai`; devDep `pkgroll` | the npm package WAS the TS pipeline. The package is now `private` — the product is the binary; `@babel/{core,traverse,types}`, `tsx`, `typescript` moved to devDependencies (the harness)                                                                                                                                                                                                                              |

### The Rust half (`rust/cutover-rust-verbs`, 16fb683, merged here)

23 migration verbs deleted — ingest, partitions, functions, modules, twins,
matches, transfers, waves, passes, naming, scope-view, rename-probe, stmtctx,
hashprobe, refprobe, vendor-names, llm-replay-gate, placement, emit, finish,
post-split-reconcile, retain-lines, prompt-gate — with every TS-dump reader
and test-only plant they alone reached (58 files, +236 / −5,326: cli −1,323,
core −3,832, parity −164). Kept: the pipeline (`humanify <input>`,
`env-reads`) and `detect`, `unpack`, `libdetect`, `format`, `format-check`.
`humanify-parity` lost `compare-ledger` and the `structuralSignature`
exemption; `selftest` + `compare` remain. Kept on purpose although only
tests reach them: `naming::prompt_gate` and `humanify-llm::replay_gate` (their
tests replay frozen captures through the LIVE prompt builders and cache-key
derivation — the warm neutrality cache depends on key stability), hence
`test/parity/wp42-gate-fixture/` and `wp41-replay/` stay.

## 5. The gate after the cutover — `scripts/check.ts` STAGES (13)

typecheck · lint · rust:fmt · rust:clippy · knip · knip:prod · census:clones
(advisory) · unit · rust:unit · **rust:build** (new: the release binary,
`--locked`) · **rust:format-golden** (new: `scripts/format-golden.sh`, the
release binary's formatter against `test/parity/format-goldens.json` + a
planted perturbation that must be detected) · rust:parity (the differ's
selftest) · **e2e** (replaced: `scripts/e2e.ts`).

Removed: `fingerprint` (retired with the TS matcher, above). `typecheck`,
`lint`, `knip`, `knip:prod`, `unit`, `census:clones` now cover the harness TS
(knip.json lists its entries and project; experiments/ is no longer ignored
wholesale).

The new `e2e`: the release binary on the committed fixtures
(`test/e2e/fixtures/{disambiguation,r1b-synthetic}`, each version pair of its
`fixture.config.json`) — fresh, then `--prior-version`, against an in-process
stub LLM that answers `<id>Renamed` for every listed identifier (so the whole
naming path runs; a run where no rename lands fails), the prior run twice for
byte-determinism, and every output imported by Node and required to export
the input's surface (names, types, the shape of each export's no-argument
result). Its first run found **#55**: the naming stage renames an ESM module's
exported bindings (`createStore` → `createStoreRenamed`). The stage reports
#55 in exactly that shape and fails on any other surface change.

## 6. What could not be retired, and what is left open

- **The Rust matcher has no npm-package ground-truth check** (the retired
  fingerprint harness/preflight's role). The Rust matching unit tests replay
  frozen specs, and the eval exercises it end to end, but a regression that
  keeps KPIs in band is invisible. Follow-up: a `humanify match` verb + a
  retargeted ground-truth harness.
- **M5's 20-hop walk segment** (`10-work-breakdown.md` M5) was not run here;
  the walk driver lives outside the repo.
- **The binary's `--help` named TS files** — RESOLVED 2026-09-26
  (fix/client-cli-bugs). `--disable` no longer points at
  `src/kill-switches.ts`, `--dump-artifacts` dropped "the parity-era
  instrument", and `--ambiguity-probe` is gone: the Rust pipeline parsed it
  and never read it (its TS instrument was re-homed harness-side and never
  ported), so it is now an unknown option. The help is pinned to the
  BINARY's own text by `test/golden/help/<command>.txt`; the recorded
  commander corpora (`wpb4-cli-surface.json`, `wpb4-scenarios.json`) keep
  the TS option table, parse outcomes and error lines, with each recorded
  help body replaced by a `{{help:<command>}}` placeholder.
- **Rust doc comments cite TS files** (`src/…`) as the provenance of ported
  code. They are history, and `PORTING.md` maps them; they were not swept.
- **`--heap-mb` stays in run.sh** though inert for the binary: every run
  manifest records `heapMb`; removing it is a manifest-schema change.
- **Finding #55** (above) is open.

## Appendix — historical experiment files deleted (dead without `src/`)

- `010-ablation/`: `run.ts`
- `011-perturbation-lab/`: `ground-truth.ts`, `minify-string.ts`, `perturbations/add-console-log.ts`, `perturbations/index.ts`, `perturbations/rename-property.ts`, `perturbations/swap-property-order.ts`, `run.ts`, `scoring.ts`
- `012-minifier-sensitivity/`: `analyze-claude-code.ts`, `diff-analysis.ts`, `discover.ts`, `inspect-ambiguous-parents.ts`, `inspect-ambiguous.ts`, `inspect-collisions.ts`, `run.ts`
- `013-bun-cjs-classification/`: `bench-rename.ts`, `classify-anomaly-mechanisms.ts`, `cross-version-baseline.ts`, `dump-banners.ts`, `inspect-hash-divergence.ts`, `library-skip-smoke.ts`, `measure-binding-match.ts`, `measure-close-match-anomaly.ts`, `smoke.ts`, `unpack-real.ts`
- `014-rename-noise-elimination/`: `fresh-pool-overlap.ts`
- `015-megafunction-truncation/`: `count-duplicate-names.ts`, `inspect-module-prompt.ts`, `simulate-windows.ts`, `truncation-coverage.ts`
- `016-diff-noise-convergence/`: `probe-enclosing-statement.ts`, `probe-parent-keys.ts`
- `020-tail-polish/`: `probe-literal-args.ts`, `run-reconcile.ts`
- `021-naming-floor/`: `census-minted-tokens.ts`, `run-floor.ts`
- `023-stable-split/`: `proto-stable-split.ts`, `run-stable-split.ts`
- `025-runnable-split/`: `emit-cjs.ts`, `probe-loadgraph.ts`, `probe-loadtime.ts`, `probe-scope.ts`, `verify-reconstruct.ts`
- `026-cjs-emit/`: `emit-cjs-live.ts`
- `028-operator-variance/`: `measure-ceiling.ts`
- `029-graph-clustering-split/`: `compare-runnable.ts`, `layout.ts`, `lib/cluster.test.ts`, `lib/cluster.ts`, `lib/folderize.test.ts`, `lib/folderize.ts`, `lib/graph.ts`, `lib/hier.ts`, `lib/io.ts`, `lib/library.test.ts`, `lib/seam.test.ts`, `lib/split.ts`, `lib/stability.test.ts`, `lib/stability.ts`, `libaware.ts`, `measure.ts`, `mqsweep.ts`, `probe-detect.ts`, `probe-unpack.ts`, `srcdist.ts`, `stability.ts`, `verify-prod.ts`, `verify-runnable.ts`
- `030-split-hash-inheritance/`: `bench-postnaming.mts`, `probe-abstains.mts`, `synthesize-hash-ledger.mts`, `validate-pair.mts`
- `031-ephemeron-per-parse/`: `bench-live-postnaming.mts`, `stress-live-main.mts`, `stress.mts`
- `032-prior-match-naming/`: `bench.mts`
- `033-naming-noise/`: `b-ceiling.ts`, `diagnose-relocations.ts`, `diagnose-v2.ts`, `diagnose-v3.ts`, `oracle-coverage.ts`, `prod-map-measure.ts`
- `034-eval-harness/`: `ceiling-identity-recovery.ts`, `ceiling-lever2.ts`
- `036-interchangeable-assignment/`: `ceiling-family-assignment.ts`
- `037-noise-source-decomposition/`: `canon-check.ts`, `decompose-noise.ts`, `disk-churn-decompose.ts`, `drift-mechanism.ts`, `emit-runnable.ts`, `leverb-sweep.sh`, `reorder-churn.ts`, `reorder-safety.ts`, `resplit-validate.ts`, `root-rename-leverage.ts`
- `038-dependency-aware-reorder/`: `align-trace.ts`, `reorder-ceiling.ts`, `run-hops.sh`, `selfhop-ledger-check.sh`, `validate-alias-fix.sh`
- `041-content-anchor/`: `ceiling.ts`, `eyeball-anchor.ts`, `preempt-ceiling.ts`, `replay-lib.ts`
- `042-anchor-preempt/`: `eyeball-preempt.ts`
- `043-name-family/`: `abstain-probe.ts`, `family-probe.ts`, `family-size.ts`, `two-witness.ts`
- `044-naming-correspondence/`: `name-drift-census.ts`, `reservation-damage.ts`
- `045-reorder/`: `barrier-exact.ts`, `probe.ts`, `pure-reorder.ts`
- `046-vendor-noise/`: `hash-probe.ts`, `predict-inherit.ts`, `why-differs.ts`
- `047-vendor-residual/`: `rename-inherit-ceiling.ts`, `verify-shipped-order.ts`
- `048-family-permute-cold/`: `byte-identity.sh`, `exact-isolation.sh`
- `049-reorder-drivers/`: `ambiguity-split.ts`, `barrier-exact-fixed.ts`, `measure-registrar.sh`, `pin-selfhop.sh`
- `050-aligner-precision/`: `gate-ceiling.ts`, `measure.sh`
- `054-post-split-reconcile/`: `audit.ts`, `carry-bisect.ts`, `carry-ceiling.ts`, `carry-diagnose.ts`, `ceiling.ts`, `classify.ts`, `pass.ts`, `read-survivors.ts`, `verify-pass.ts`
- `055-residual-recount/`: `full-ledger.ts`, `name-drivers.ts`, `one-sided-ledger.ts`
- `056-multi-hop-walk/`: `walk.sh`
- `057-alias-stability/`: `ceiling.ts`, `hash-collision-probe.ts`, `moved-ren-identity.ts`, `ns-classify.ts`, `position-signal.ts`, `show-pairs.ts`, `trail-check.ts`
- `058-binding-placement/`: `ceiling-ab.ts`, `disagree.ts`, `read-disagreements.ts`, `reloc-witness.ts`, `rule-a-moves.ts`, `singleton-census.ts`, `singleton-guard-probe.ts`, `trail-dump.ts`, `trail-inert.ts`
- `059-rename-capture/`: `repro.sh`
- `061-hidden-name-churn/`: `apply-mixed.ts`, `loc-provenance.ts`, `name-only-mechanisms.ts`, `tier-provenance.ts`
- `062-duplicate-instances/`: `census.ts`
- `063-name-contention/`: `ceilings.ts`
- `064-edit-pair-matcher/`: `ceiling.ts`
- `065-selfhop-unmatched/`: `census.ts`
- `069-naming-stability/`: `ceilings.ts`, `reach.ts`
- `070-fossil-split/`: `ceiling.ts`, `preview.ts`, `vendor-census.ts`
- `073-identity-carry/`: `addressable.ts`, `ceiling.ts`, `diagnose.ts`
- `074-path-stability/`: `ceiling.mts`, `match-probe.mts`, `verify.mts`
- `076-statement-placement/`: `churn.ts`, `collapse-sweep.ts`, `folder-shape.ts`, `naming-evidence.ts`, `stability.ts`, `walk.sh`
- `078-durable-names/`: `graded-similarity.ts`, `task0-attribute.ts`
- `079-ambiguity/`: `abstain-hop.sh`, `holder-group-census.ts`, `indirection-census.ts`, `stmt-size-census.ts`
- `080-noise-sources/`: `close-match-headroom.ts`
