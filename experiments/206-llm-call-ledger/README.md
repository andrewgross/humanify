# exp206 — the LLM call ledger: every request's fate, per namer, per run

STATUS: IN PROGRESS (2026-11-08), branch `exp206-llm-call-ledger`.

Andrew, 2026-11-08: "we should be tracking errors, response time etc in our
debugging, and we can pull that out to understand the number of requests
that truly failed, needed a backup strategy ... so we can understand changes
due to server speed, vs the model replicating names, vs other failures".

## Step 1 — census (before this lane)

| what                                        | where it was recorded                                                                                                  | reached stats / manifest / card?                                     |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| calls failed after every retry, per namer   | `providerErrors` (cli `provider_failures.rs`)                                                                          | stats, run-status, summary refusal banner, WARNING line — COMPLETE   |
| latency per call                            | `-vv` log only (`[LLM] suggestAllNames - SUCCESS (2742ms)`, every call logged TWICE: client + debug wrapper)           | no                                                                   |
| coverage `retries` / `avgResponseTimeMs`    | HARD-CODED `0` / `0` in core `naming/driver.rs` `coverage_inputs`; tokens always absent                                | stats — FALSE ZEROS on every Rust-era card (below)                    |
| `MetricsTracker` (calls/failed/retries/avg) | llm `metrics.rs`, fed by the limiter                                                                                   | never: the CLI passed `metrics: None` — never wired                  |
| timeouts / connection errors / HTTP status  | only a call's FINAL error (ERROR block in `-vv`, then a providerErrors count); the 3 SDK + 4 limiter attempts silent   | no                                                                   |
| retries and why                             | nowhere                                                                                                                | no                                                                   |
| malformed completion bodies                 | a failed call (indistinguishable); invalid-JSON content falls back to a regex silently                                 | as a failure only                                                    |
| truncation (`finish_reason: length`)        | drives the lane's batch shrink; never counted                                                                          | no                                                                   |
| tokens                                      | in each response; nothing summed them                                                                                  | no                                                                   |
| memo / disk cache                           | memo asked/shared: one stdout line; disk hits/misses/writes: stdout; manifest counts cache files before/after          | manifest `cache.written` only                                         |
| answers rejected by reason (lane)           | per-identifier trails in diag.json (`strategyTrails`, `identifierLedger`); aggregate nowhere                           | `reask.lateRejections` / `allFailedWindows` / `invalidSuggestionFinishes` only |
| backups taken                               | hand-offs counted in the processor report, never exported; ladder decorations in diag.json `nameContention` only       | sweep re-asks (`reask.sweep*`), `promptGuard.retryAsked`              |
| model repeating the prior's name            | nowhere                                                                                                                | no                                                                   |
| why each ask happened                       | `--dump-asks` (ask trace)                                                                                              | no                                                                   |

Gaps against the three questions: (a) server speed and health — nothing;
(b) the model's answers — partial; (c) what truly failed — counted, but with
no reason.

## What was built

(filled in at the end)
