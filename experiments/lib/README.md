# experiments/lib — the shared measurement library

Everything a gate or a ceiling needs, once. Experiment directories hold only
what is specific to that experiment.

Since the cutover (2026-09-26, `docs/rust-port/19-cutover.md`) the pipeline
is the Rust binary; this library measures it and imports nothing from a
pipeline. The few TS pieces the scorer needed from the deleted `src/` live in
`js/`, owned here.

| file                      | what it owns                                                                                                                                                                                        |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `diff.ts`                 | the ONE changed-line counter. Normal `diff`, `<`/`>`, `-rN` for trees. A modified line counts twice, which is what every published "git lines" figure here means. Fails loudly if `diff` cannot run |
| `trees.ts`                | walking a tree, reading its ledger / bundle / match-map, and deriving statements. `bundleStatements` and `fileStatements` DECLARE which input shape they want and throw on the other                |
| `js/statement-hash.ts`    | THE KPI HASH (`statementHash`), byte-for-byte the pre-cutover code — re-scoring three labels proved the cards identical. Never edit it without re-scoring every reference                           |
| `js/babel.ts`             | the harness's Babel parse/traverse funnel (Babel is a harness devDependency)                                                                                                                        |
| `js/wrapper.ts`           | finding a bundle's wrapper IIFE (for `bundleStatements`)                                                                                                                                            |
| `js/tree-layout.ts`       | `.humanify/`, the tree walk, the split-ledger read type                                                                                                                                             |
| `js/line-diff.ts`         | `computeNormalDiff` (under `diff.ts`) and the line tokenizer the `eval diff` ledger uses                                                                                                            |
| `js/structural-tokens.ts` | the name-masked token stream (vendor churn scoring, the clone census)                                                                                                                               |
| `pipeline-bin.ts`         | which binary scored a label: builds it, records sha + commit, refuses a mismatch                                                                                                                    |
| `run-pipeline.ts`         | the ONE launcher of a scored run, and its manifest                                                                                                                                                  |
| `build-bin.sh`            | building the binary for a shell instrument's leg                                                                                                                                                    |
| `boot-gate.sh`            | the boot gate. FATAL when `bun` is missing                                                                                                                                                          |
| `gate.sh`, `selfhop.sh`   | the draw-pinned A/B and the tree-level self-hop (the binary, built from this checkout)                                                                                                              |
| `neutrality.sh`           | the byte-identity gate: each leg's binary built from its own commit                                                                                                                                 |

Retired at the cutover (they drove the TS pipeline in-process, and git history
at tag `m4` holds them): `counterfactual.ts` + `verify-counterfactual.ts`
(git-capped ceilings through the TS splitter/emitter), the three `size-*.ts`
sizers, and `matcher-preflight.sh` (validated the TS matcher; a Rust-matcher
equivalent is an open follow-up in 19-cutover.md).

## What knip audits here

Every top-level `experiments/lib/*.ts` is a knip ENTRY (knip.json): their
callers are experiment scripts knip does not scan, so as ordinary files they
would all read as dead. As entries, knip still audits everything they import —
`js/` above all — for dead exports.

## What is NOT here yet

`pairs.json` is still read three ways (two shell, one TS) and only the shell
paths honour `EVAL_ENDPOINT` / `EVAL_INPUTS_BASE`. Cache-dir variables are still
per-script (`GATE_CACHE`, `SELFHOP_CACHE`, `ISOLATION_CACHE`, two hard-coded).
