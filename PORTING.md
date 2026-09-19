# PORTING.md — the porting ledger

**The single place porting status is visible** (docs/rust-port/10-work-breakdown.md §4). Commit messages, chat and other docs must not claim status independently.

Oracle label in force: **none yet** (WP0.4 sets it). Oracle commit: **none yet**.

**TOTAL 44,200 LOC / parity-green 0 LOC (0.0%)**

Statuses: `not-started` / `in-progress` / `parity-green` / `replaced` / `designed-out` / `subprocess` / `harness-side`. **No status change without a gate-run citation** (command + oracle label + date, or a results path). `in-progress` rows carry the date and session that claimed them.

Seeded 2026-09-19 by the migration structure owner from `find src -name "*.ts" ! -name "*.test.ts" ! -name "*.e2etest.ts"` (142 files, 44,200 LOC; doc 10 §1 counted 143 / 44,393 on 2026-08-27 — the difference is drift since, re-verify at WP0.1) and the WP contents columns of doc 10 §2. 6 files matched no WP row and are marked UNASSIGNED; WP0.1 assigns them and fills the Rust-module column for every row.

| TS file                               |   LOC | WP    | Rust module | status      | gate run                                   |
| ------------------------------------- | ----: | ----- | ----------- | ----------- | ------------------------------------------ |
| rename/strategy-trail.ts              |   185 | WP0.2 |             | not-started |                                            |
| analysis/structural-hash.ts           | 1,103 | WP1.3 |             | not-started |                                            |
| analysis/function-graph.ts            |   985 | WP1.4 |             | not-started |                                            |
| analysis/types.ts                     |   726 | WP1.4 |             | not-started |                                            |
| analysis/bun-module-classification.ts |   626 | WP1.5 |             | not-started |                                            |
| debug.ts                              |   537 | WP1.1 |             | not-started |                                            |
| analysis/function-fingerprint.ts      |   394 | WP1.4 |             | not-started |                                            |
| analysis/known-globals.ts             |   269 | WP1.5 |             | not-started |                                            |
| babel-utils.ts                        |   245 | WP1.2 |             | not-started |                                            |
| kill-switches.ts                      |   191 | WP1.1 |             | not-started |                                            |
| llm/types.ts                          |   149 | WP1.4 |             | not-started |                                            |
| analysis/wrapper-detection.ts         |   119 | WP1.5 |             | not-started |                                            |
| split/statement-hash.ts               |   114 | WP1.3 |             | not-started |                                            |
| analysis/enclosing-statement.ts       |   101 | WP1.3 |             | not-started |                                            |
| profiling/types.ts                    |    98 | WP1.4 |             | not-started |                                            |
| analysis/soundness.ts                 |    90 | WP1.5 |             | not-started |                                            |
| verbose.ts                            |    58 | WP1.1 |             | not-started |                                            |
| unpack/index.ts                       |    57 | WP1.1 |             | not-started |                                            |
| detection/types.ts                    |    53 | WP1.4 |             | not-started |                                            |
| commands/default-args.ts              |    52 | WP1.1 |             | not-started |                                            |
| unpack/types.ts                       |    49 | WP1.4 |             | not-started |                                            |
| utils/concurrency.ts                  |    44 | WP1.1 |             | not-started |                                            |
| file-utils.ts                         |    40 | WP1.1 |             | not-started |                                            |
| library-detection/types.ts            |    37 | WP1.4 |             | not-started |                                            |
| pipeline/types.ts                     |    21 | WP1.4 |             | not-started |                                            |
| profiling/index.ts                    |    20 | WP1.1 |             | not-started |                                            |
| library-detection/index.ts            |    19 | WP1.1 |             | not-started |                                            |
| index.ts                              |    16 | WP1.1 |             | not-started |                                            |
| utils/identifier-regex.ts             |    13 | WP1.1 |             | not-started |                                            |
| cli.ts                                |    10 | WP1.1 |             | not-started |                                            |
| shared/regex.ts                       |     9 | WP1.1 |             | not-started |                                            |
| env.ts                                |     8 | WP1.1 |             | not-started |                                            |
| number-utils.ts                       |     7 | WP1.1 |             | not-started |                                            |
| cli-error.ts                          |     4 | WP1.1 |             | not-started |                                            |
| detection/index.ts                    |     2 | WP1.1 |             | not-started |                                            |
| prior-version/prior-version.ts        | 1,860 | WP2.4 |             | not-started |                                            |
| analysis/fingerprint-index.ts         | 1,463 | WP2.1 |             | not-started |                                            |
| prior-version/statement-twin.ts       | 1,219 | WP2.3 |             | not-started |                                            |
| prior-version/statement-align.ts      |   539 | WP2.2 |             | not-started |                                            |
| analysis/close-match.ts               |   258 | WP2.2 |             | not-started |                                            |
| prior-version/binding-role.ts         |   239 | WP2.3 |             | not-started |                                            |
| prior-version/ambiguity-probe.ts      |   107 | WP2.4 |             | not-started |                                            |
| rename/lifecycle.ts                   |   101 | WP2.4 |             | not-started |                                            |
| rename/prior-transfer.ts              | 1,914 | WP3.2 |             | not-started |                                            |
| rename/validated-rename.ts            |   568 | WP3.1 |             | not-started |                                            |
| analysis/propagation.ts               |   457 | WP3.3 |             | not-started |                                            |
| rename/function-bindings.ts           |   228 | WP3.2 |             | not-started |                                            |
| rename/rename-ledger.ts               |   158 | WP3.1 |             | not-started |                                            |
| rename/proximity.ts                   |   143 | WP3.3 |             | not-started |                                            |
| rename/single-vote-pin.ts             |   127 | WP3.3 |             | not-started |                                            |
| rename/skip-list.ts                   |    92 | WP3.2 |             | not-started |                                            |
| split/prior-carry.ts                  |    61 | WP3.2 |             | not-started |                                            |
| rename/rename-eligibility.ts          |    60 | WP3.1 |             | not-started |                                            |
| rename/carried-names.ts               |    59 | WP3.2 |             | not-started |                                            |
| rename/processor.ts                   | 3,224 | WP4.3 |             | not-started |                                            |
| rename/diff-reconcile.ts              | 1,813 | WP4.4 |             | not-started |                                            |
| rename/plugin.ts                      | 1,677 | WP4.6 |             | not-started |                                            |
| llm/prompts.ts                        |   507 | WP4.2 |             | not-started |                                            |
| rename/family-permute-step.ts         |   414 | WP4.5 |             | not-started |                                            |
| rename/diagnostics.ts                 |   402 | WP4.5 |             | not-started |                                            |
| rename/minted-census.ts               |   398 | WP4.5 |             | not-started |                                            |
| rename/coverage.ts                    |   386 | WP4.3 |             | not-started |                                            |
| llm/metrics.ts                        |   365 | WP4.1 |             | not-started |                                            |
| rename/wave-scheduler.ts              |   325 | WP4.3 |             | not-started |                                            |
| llm/openai-compatible.ts              |   286 | WP4.1 |             | not-started |                                            |
| rename/coverage-sweep.ts              |   281 | WP4.5 |             | not-started |                                            |
| rename/code-window.ts                 |   239 | WP4.2 |             | not-started |                                            |
| llm/rate-limiter.ts                   |   231 | WP4.1 |             | not-started |                                            |
| llm/validation.ts                     |   218 | WP4.2 |             | not-started |                                            |
| rename/context-builder.ts             |   206 | WP4.2 |             | not-started |                                            |
| rename/family-permute.ts              |   185 | WP4.5 |             | not-started |                                            |
| llm/cached-provider.ts                |   162 | WP4.1 |             | not-started |                                            |
| rename/reconcile-step.ts              |   131 | WP4.4 |             | not-started |                                            |
| rename/class-id-floor.ts              |   114 | WP4.5 |             | not-started |                                            |
| rename/sweep-step.ts                  |   110 | WP4.5 |             | not-started |                                            |
| rename/prior-name-snap.ts             |   104 | WP4.5 |             | not-started |                                            |
| rename/wave-profile.ts                |    86 | WP4.3 |             | not-started |                                            |
| rename/decoration-retry.ts            |    79 | WP4.5 |             | not-started |                                            |
| llm/debug-wrapper.ts                  |    62 | WP4.1 |             | not-started |                                            |
| split/cjs-emit.ts                     | 1,678 | WP5.3 |             | not-started |                                            |
| split/stable-split.ts                 | 1,673 | WP5.1 |             | not-started |                                            |
| split/cluster-assign.ts               |   981 | WP5.2 |             | not-started |                                            |
| split/fossil-assign.ts                |   881 | WP5.2 |             | not-started |                                            |
| split/load-order.ts                   |   670 | WP5.3 |             | not-started |                                            |
| split/fossil-match.ts                 |   653 | WP5.2 |             | not-started |                                            |
| split/post-split-reconcile.ts         |   502 | WP5.4 |             | not-started |                                            |
| split/bundle-carry.ts                 |   344 | WP5.4 |             | not-started |                                            |
| split/fossil-map.ts                   |   324 | WP5.2 |             | not-started |                                            |
| split/runnable-scaffold.ts            |   317 | WP5.4 |             | not-started |                                            |
| split/bun-relink.ts                   |   277 | WP5.4 |             | not-started |                                            |
| split/placement-trail.ts              |   231 | WP5.1 |             | not-started |                                            |
| split/content-anchor.ts               |   224 | WP5.1 |             | not-started |                                            |
| split/split-namer.ts                  |   203 | WP5.2 |             | not-started |                                            |
| plugins/babel/babel.ts                |   151 | WP5.6 |             | not-started |                                            |
| split/vendor-body-inherit.ts          |   144 | WP5.4 |             | not-started |                                            |
| split/layout.ts                       |   119 | WP5.1 |             | not-started |                                            |
| shared/bun-helpers.ts                 |   111 | WP5.3 |             | not-started |                                            |
| split/using-desugar.ts                |    96 | WP5.4 |             | not-started |                                            |
| shared/cjs-factory.ts                 |    67 | WP5.3 |             | not-started |                                            |
| split/emitter.ts                      |    55 | WP5.3 |             | not-started |                                            |
| split/substitutions.ts                |    50 | WP5.3 |             | not-started |                                            |
| shared/unique-name.ts                 |    27 | WP5.3 |             | not-started |                                            |
| plugins/babel/beautifier.d.ts         |     5 | WP5.6 |             | not-started |                                            |
| commands/unified.ts                   | 1,629 | WPB.4 |             | not-started |                                            |
| unpack/adapters/bun.ts                |   827 | WPB.2 |             | not-started |                                            |
| output-validation.ts                  |   401 | WPB.4 |             | not-started |                                            |
| ui/progress.ts                        |   330 | WPB.4 |             | not-started |                                            |
| env-reads/analyze.ts                  |   260 | WPB.4 |             | not-started |                                            |
| unpack/vendor-namer.ts                |   254 | WPB.2 |             | not-started |                                            |
| profiling/profiler.ts                 |   203 | WPB.5 |             | not-started |                                            |
| unpack/manifest-order.ts              |   198 | WPB.2 |             | not-started |                                            |
| unminify.ts                           |   187 | WPB.4 |             | not-started |                                            |
| commands/settings.ts                  |   162 | WPB.4 |             | not-started |                                            |
| library-detection/adapters/bun.ts     |   160 | WPB.2 |             | not-started |                                            |
| library-detection/adapters/default.ts |   160 | WPB.3 |             | not-started |                                            |
| detection/signals/minifier.ts         |   141 | WPB.1 |             | not-started |                                            |
| library-detection/comment-regions.ts  |   137 | WPB.3 |             | not-started |                                            |
| plugins/webcrack.ts                   |   127 | WPB.2 |             | not-started |                                            |
| failed-output.ts                      |   101 | WPB.4 |             | not-started |                                            |
| profiling/trace-events.ts             |    97 | WPB.5 |             | not-started |                                            |
| env-reads/format.ts                   |    81 | WPB.4 |             | not-started |                                            |
| detection/detect.ts                   |    77 | WPB.1 |             | not-started |                                            |
| profiling/summary.ts                  |    64 | WPB.5 |             | not-started |                                            |
| commands/env-reads.ts                 |    60 | WPB.4 |             | not-started |                                            |
| detection/signals/bun.ts              |    53 | WPB.1 |             | not-started |                                            |
| pipeline/config.ts                    |    51 | WPB.4 |             | not-started |                                            |
| pipeline/selection-record.ts          |    41 | WPB.4 |             | not-started |                                            |
| detection/signals/browserify.ts       |    37 | WPB.1 |             | not-started |                                            |
| library-detection/banner-patterns.ts  |    34 | WPB.3 |             | not-started |                                            |
| stage-fingerprint.ts                  |    27 | WPB.4 |             | not-started |                                            |
| unpack/adapters/passthrough.ts        |    22 | WPB.2 |             | not-started |                                            |
| detection/signals/pattern-helper.ts   |    21 | WPB.1 |             | not-started |                                            |
| unpack/adapters/webcrack.ts           |    17 | WPB.2 |             | not-started |                                            |
| detection/signals/esbuild.ts          |    14 | WPB.1 |             | not-started |                                            |
| detection/signals/parcel.ts           |    14 | WPB.1 |             | not-started |                                            |
| detection/signals/webpack.ts          |    12 | WPB.1 |             | not-started |                                            |
| analysis/analysis-cache.ts            |    82 | WP0.1 |             | in-progress | claim 2026-09-19, wp0.1 implementing agent |
| rename/name-contention.ts             |    56 | WP0.1 |             | in-progress | claim 2026-09-19, wp0.1 implementing agent |
| rename/library-prefix-resolver.ts     |    48 | WP0.1 |             | in-progress | claim 2026-09-19, wp0.1 implementing agent |
| rename/prior-match-map.ts             |    46 | WP0.1 |             | in-progress | claim 2026-09-19, wp0.1 implementing agent |
| rename/run-config.ts                  |    39 | WP0.1 |             | in-progress | claim 2026-09-19, wp0.1 implementing agent |
| rename/graph-closure.ts               |    38 | WP0.1 |             | in-progress | claim 2026-09-19, wp0.1 implementing agent |

**REMAINING: 44,200 LOC not parity-green**
