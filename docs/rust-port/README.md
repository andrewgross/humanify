# Rust port plan

**Status: PROPOSAL — no build started, no decision made.** Docs 01–04 written
2026-08-27 from the current tree (`main` + `feat/electron-unpacking` for the
Electron notes) plus measured run data; the deep-planning set 05–11 added
2026-08-28, drafted and repo-fact-checked by parallel agents; 12 (layout and
diff architecture, plus the compatibility posture in 02 §9) added the same
day. Every "today" number cites its source.

## The pitch in one paragraph

A cross-version hop is ~85% single-threaded CPU — measured, not assumed: LLM
wall time at concurrency 32 is 75–135 s inside 9–17 minute runs, and even the
"LLM phase" runs at 7.3× effective parallelism against a 32 ceiling because
the single-threaded process cannot feed the server. A Rust pipeline on the
oxc toolchain (whose semantic model — pre-resolved `SymbolId` references in
flat tables — is natively the rename-invariant identity this project builds
by hand on Babel) is projected to take a big-pair hop from ~17 min to
~2.5–3.5 min, warm verification runs down ~10×, and peak memory from 15–30 GB
(64 GB heap) to a few GB. The algorithms — the cascade, the guards, the
naming tiers, the placement tiers — port unchanged; the migration is designed
so decisions are proven equivalent before any text changes, using the
existing eval/neutrality instruments as the judge.

## Reading order

| doc                                                                  | what it holds                                                                                                                                                                                                  |
| -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`01-current-architecture.md`](./01-current-architecture.md)         | what the tool is: intent, the 12 stages, the identity system, the cascades, the discipline, the measurement stack, the machine's measured shape                                                                |
| [`02-rust-target-architecture.md`](./02-rust-target-architecture.md) | the target design: oxc mapping, crate shape, the two deliberate changes (formatter-independent hashing, provenance sidecar), determinism and parallelism models, what stays TS                                 |
| [`03-migration-plan.md`](./03-migration-plan.md)                     | the conversion: hold text fixed → port decisions → verify exactly → swap the formatter once; phase gates, risks, early falsifiers                                                                              |
| [`04-performance-model.md`](./04-performance-model.md)               | the measured time budget and the projected savings, with confidence labels and the cheap next measurement                                                                                                      |
| [`05-rust-toolchain.md`](./05-rust-toolchain.md)                     | the Rust development stack: workspace layout, toolchain pins, the clippy policy mapped rule-by-rule to house rules, dependency policy, build profiles, the dev loop, and how cargo stages join `npm run check` |
| [`06-testing-strategy.md`](./06-testing-strategy.md)                 | the test taxonomy: colocated units, snapshot families, property tests tied to past incidents, mutation testing over the guard modules, fixtures shared with the TS suite, doc-drift guards                     |
| [`07-differential-validation.md`](./07-differential-validation.md)   | the confidence machine: artifact schemas and join keys, the parity comparer, prompt and cache-key bit-parity, cross-implementation neutrality, divergence adjudication, oracle lifecycle                       |
| [`08-operations-runbook.md`](./08-operations-runbook.md)             | building, running, observing, debugging: CLI compatibility table, env and kill switches, preserved log contracts, eval/walk/devcontainer integration, failure behavior                                         |
| [`09-experiment-methodology.md`](./09-experiment-methodology.md)     | measurement in the Rust era: what stays TypeScript, what accelerates, the new instruments (phase-time KPI, criterion benches), ablation with mandatory trails, a worked post-port lever example                |
| [`10-work-breakdown.md`](./10-work-breakdown.md)                     | the execution plan: work packages with gates and dependencies, parallel lanes, the porting ledger, go/no-go milestones, calendar bands                                                                         |
| [`11-risk-register.md`](./11-risk-register.md)                       | every known risk with a cheap early probe that sizes it before it can hurt                                                                                                                                     |
| [`12-layout-and-diff.md`](./12-layout-and-diff.md)                   | the version record (prior as tables, never an AST), the layout plan, the virtual diff as in-run scorecard and reconcile evidence — and why it is never an objective                                            |

## The decision this asks for

Whether to run **phases 0–2** of the migration plan (contract + oracle dumps
in TS, then the Rust core model and matching cascade, gated on exact decision
parity). That is the fraction-of-total-cost experiment that proves or
falsifies the approach; phase 2's exact-match gate is the go/no-go milestone.
Independent of the port, two items in these docs pay for themselves anyway:

- enabling the existing-but-never-enabled span profiler for one eval sweep
  (turns the phase-time table from log reconstruction into a trace), and
- testing whether the local vLLM server sustains more than 32 concurrent
  requests (the LLM wall is the post-port floor, and possibly today's
  cheapest win).
