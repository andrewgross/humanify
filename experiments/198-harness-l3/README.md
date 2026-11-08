# exp198 — harness migration lane L3: every runner in Rust

STATUS: IN PROGRESS (2026-11-07), branch `harness-l3-runners`. Scope is
`docs/dev-harness.md` §5 lane L3 (the runners), on top of lane L1's crate
(`crates/humanify-ag-dev`), with Andrew's 2026-11-07 decisions (the walk is
ported; the eval KPI scorer is a new scorer version, lane L2a).

## Totals

PENDING — written when the proof runs finish.

## 1. What was ported (old path -> verb)

| Old (TS / sh / Python)                                                                                         | New (`cargo dev ...`, module)                                                          | Notes                                                                                                                            |
| -------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- | -------- |
| `scripts/eval.ts` verb `score`                                                                                 | `eval score <label>` (`runners::score`, `runners::verbs`)                              | flags are L1's `HARNESS_KEYS`; `--seeded-base` / `--archive-prior` are `--base-mode seeded                                       | archive` |
| `experiments/034-eval-harness/run.sh`                                                                          | `eval score` (`runners::score`)                                                        | rebase (scratch / seeded / archive), scored leg, run status + refusals, card, evidence page, boot, cold + warm self-hop, summary |
| `experiments/lib/pipeline-bin.ts`                                                                              | `runners::bin` (`build_and_describe`, `bin_commit_refusal`) on L1's `bin_build`        |                                                                                                                                  |
| `experiments/lib/pipeline-config.ts`                                                                           | `launch` (`render_config`, `write_config`, `HumanifyArgs`)                             |                                                                                                                                  |
| `experiments/lib/run-pipeline.ts`                                                                              | `runners::leg` (`run_leg`)                                                             | RSS sampled from /proc while the child lives (`proc::run_to_file`)                                                               |
| `experiments/lib/run-manifest.ts` (writer, `manifestWarnings`, `priorKindOf`, `baseModeOf`)                    | `runners::records`                                                                     | the readers for comparison (`labelBaseModes`, `namingModeOf`, the comparisons) are L1's identity backfill / lane L2a             |
| `experiments/lib/invariants.ts` (writer side: `writeRunStatus`, both refusals)                                 | `runners::records`                                                                     | the reader side (`loadRunStatuses`, banners, `coldRangeVerdict`) is the summary's: lane L2a                                      |
| `experiments/lib/llm-endpoint.sh`                                                                              | L1's `committed::local::resolve_endpoint` (used by every runner)                       |                                                                                                                                  |
| `scripts/eval.ts` verb `neutrality` + `experiments/lib/neutrality.sh` + `experiments/lib/build-bin.sh`         | `eval neutrality <ref> [from:to]` (`runners::neutrality`)                              | legacy no-`--config` leg kept                                                                                                    |
| `experiments/lib/boot-gate.sh`                                                                                 | `boot-gate <tree> <version>` (`runners::boot`)                                         |                                                                                                                                  |
| `experiments/096-library-matching-score/run.py` (launch half: build, `write_rule_config`, `run_one`, the pool) | `runners::library` (`build`, `write_rule_config`, `run_discover`)                      | the `library score` VERB, the corpus and the scoring are lane L2b's; it calls these                                              |
| `experiments/166-restore-or-ask/stub-run.mts`                                                                  | `run stub <bin> <bundle> <out> [<prior>]` (`runners::stub`, on the testkit's one stub) |                                                                                                                                  |
| `scripts/build-index.sh` (branch `exp184-index-spec`, never on main)                                           | `index build <spec>` (`runners::index`)                                                | on today's `humanify-ag index build`                                                                                             |
| `experiments/walk/walk.sh`                                                                                     | `eval walk <from>..<to> --walk-dir D` (`runners::walk`)                                | rust only (the dead `--pipeline ts` mode is gone)                                                                                |
| `experiments/walk/report.ts`, `hunks.ts`, `history.sh` (+ `hunks.test.ts`)                                     | `eval walk-report <name>=<dir>...` (`runners::walk::{report,hunks,history}`)           | + the new "Hops to look at" section                                                                                              |
| `experiments/034-eval-harness/run-launch.test.ts`                                                              | `runners::score_test`                                                                  |                                                                                                                                  |
| `test/launch-goldens.test.ts` (neutrality, library, boot parts)                                                | `runners::{neutrality,library}_test`, `tests/boot_gate.rs`                             | ask-trace / match-truth parts are lane L4's                                                                                      |
| `test/eval-score-args.test.ts` (score / neutrality parts)                                                      | `runners::config_test`, `runners::score_test` (label guards)                           | the library-args part is L2b's                                                                                                   |
| `test/boot-gate-env.test.ts`                                                                                   | `tests/boot_gate.rs` (the scrub, recorded)                                             |                                                                                                                                  |

NOT ported (no living caller, experiment-specific orchestration; retire
with their experiments): `experiments/152-full-bun-index/scripts/{build-full.sh,
make_shims.sh, timed.sh}`, `step_times.py`, `root_roles.py` and
`experiments/181-index-all-bundlers/scripts/ix.sh` (non-spec builds with
pinned tool paths; the spec-driven `index build` replaces the way an index
is built).

## 2. Proof

PENDING.

## 3. Gate

PENDING.

## 4. What changed from the design, and why

PENDING.

## 5. What I could not prove

PENDING.
