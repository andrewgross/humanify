# experiments/lib — the shared measurement library

Everything a gate or a ceiling needs, once. Experiment directories hold only
what is specific to that experiment.

| file                       | what it owns                                                                                                                                                                                        |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `diff.ts`                  | the ONE changed-line counter. Normal `diff`, `<`/`>`, `-rN` for trees. A modified line counts twice, which is what every published "git lines" figure here means. Fails loudly if `diff` cannot run |
| `trees.ts`                 | walking a tree, reading its ledger / bundle / match-map, and deriving statements. `bundleStatements` and `fileStatements` DECLARE which input shape they want and throw on the other                |
| `counterfactual.ts`        | git-capped ceilings: perturb a copy of the prior ledger, run the REAL splitter and emitter, re-diff. `fidelity` is mandatory                                                                        |
| `verify-counterfactual.ts` | proves the harness still reproduces exp058's published numbers                                                                                                                                      |
| `boot-gate.sh`             | the boot gate. FATAL when `bun` is missing                                                                                                                                                          |
| `gate.sh`, `selfhop.sh`    | the draw-pinned A/B and the tree-level self-hop                                                                                                                                                     |

## Why knip does not audit this directory

`knip.json` ignores `experiments/**`, which includes this library. That is
deliberate and cannot be fixed by un-ignoring it: these exports are consumed by
ad-hoc experiment scripts under the ignored paths, so knip sees a library with
no callers and reports every export as dead. Un-ignoring `experiments/lib`
alone makes `readLedger`, `readBundle` and `readMatchMap` look unused when
`counterfactual.ts` imports all three.

The exports here are covered instead by `lib.test.ts` and by
`verify-counterfactual.ts`, both of which run in `npm run check`.

## What is NOT here yet

`pairs.json` is still read three ways (two shell, one TS) and only the shell
paths honour `EVAL_ENDPOINT` / `EVAL_INPUTS_BASE`. Cache-dir variables are still
per-script (`GATE_CACHE`, `SELFHOP_CACHE`, `ISOLATION_CACHE`, two hard-coded).
