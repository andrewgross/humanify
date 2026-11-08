# exp206 — the LLM call ledger: every request's fate, per namer, per run

STATUS: DONE (2026-11-08), branch `exp206-llm-call-ledger`, merged `--no-ff` to main after the worktree gate and a clean-checkout gate of the merge commit.

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

- `LlmCall.site` (`CallSite`: naming / modules / vendor / split /
  treeReviser) — outside the request, so cache keys, prompts and dumps are
  unchanged.
- ONE owner, `humanify_ag_model::call_ledger::CallLedger` (categories:
  `classify_sent`, `judge_answer`). Fed by the provider stack: the answer
  memo files every call's outcome and what it served; the limiter files
  each sent call; the client pushes each HTTP attempt (both retry layers)
  into the call's `provider::CallTrace`. It replaced the never-wired
  `MetricsTracker` as the limiter's one counter set.
- `--stats-json` `llmCalls` (outcomes / transport / timing, per namer +
  total); the coverage block's `retries`, `avgResponseTimeMs` and tokens
  now come from the ledger; `reask` gains `collisionHandoffs`,
  `laneEndHandoffs`, `ladderAtLaneEnd`, `ladderAtBarrier`.
- The eval (`humanify-ag-dev` `eval::llm_diag`): `eval summarize` writes
  `summary.json` `llmDiagnostics` (one digest per pair, from the label's
  `<to>.stats.json`) and prints SERVER / MODEL ANSWERS / BACKUPS AND
  FAILURES tables per pair; `eval leaderboard` prints them side by side over
  the compared pairs with deltas against the baseline ("p90ms 2000 vs 6800
  (+4800)"), labelled DIAGNOSTICS (no band). Legacy labels print `-`. The
  verb table says what they prove and cannot prove.
- Docs: responsibility.md (two rows), concurrency.md (R1 now measurable),
  concurrency allowlist (+3 short-lock entries).

Found while sampling: the identifier rules must not judge the split /
module / vendor / reviser answers (file, folder, package names) — a first
cold run filed 6 of 7 split answers as `invalid`. Fixed (red/green): the
`invalid` verdict applies to the `naming` site only.

## Sample breakdown — a real cold run

The release binary on the e2e fixture `bun-mixed` (v1.0.0), against the
:8000 gpt-oss-20b server, no cache, library matching off. **Latency was
measured while exp200's eval was loading the same server: not a clean
baseline.**

| view                | naming          | split | vendor | treeReviser | total           |
| ------------------- | --------------- | ----- | ------ | ----------- | --------------- |
| calls / answered    | 272 / 272       | 2 / 2 | 1 / 1  | 1 / 1       | 276 / 276       |
| failed              | 0               | 0     | 0      | 0           | 0               |
| asked identifiers   | 545             | 7     | 2      | 1           | 555             |
| proposed            | 541             | 7     | 2      | 1           | 551             |
| duplicate in answer | 4               | 0     | 0      | 0           | 4               |
| sent / first try    | 272 / 272       | 2 / 2 | 1 / 1  | 1 / 1       | 276 / 276       |
| retried attempts    | 0               | 0     | 0      | 0           | 0               |
| tokens in / out     | 195,375 / 22,307 |      |        |             | 197,996 / 22,866 |
| attempt ms p50/p90/p99/max | 424 / 774 / 2,107 / 2,196 | 499 / 545 | 800 | 364 | 424 / 779 / 2,107 / 2,196 |

coverage.llm now reads `retries 0, avgResponseTimeMs 527, totalTokens
220,862` (was hard-coded `0 / 0 / absent`). A first run of the same
fixture a few minutes earlier read p50 391 / p90 509 ms — the server's
load moved, which is exactly what the timing view is for.

## Neutrality proof

`HELD_LOCK=1 cargo dev eval neutrality main 2.1.85:2.1.86 --workdir
/work/exp206-neutrality --priors /work/exp050-cold --cache
/work/neutrality-cache` (names mode, full prebuilt index — the mode the
standing cache was last filled in), candidate d6a1ce95c vs main 21a8e21f0:

- cache writes: candidate +0, **baseline +0** (both legs fully warm: "0 sent
  to the model, 484 identical copies served")
- exit codes 0 / 0
- **differing files 0, differing lines 0 — NEUTRAL**

The one change after that commit (the `invalid` verdict's site scope) is
inside `call_ledger.rs`, which the pipeline cannot read (the
`call_ledger_write_only` guard test: core never names the ledger; its read
methods are called only in `write_stats_json`; checked to fire).

## What this cannot prove

- The answer verdicts judge the RAW answer. Whether the pipeline then kept
  a `proposed` name (scope rejection, barrier re-ask, ladder) is the
  `reask` block's record, not the ledger's.
- Between two COLD runs the answer columns move with the model's draws as
  well as with the change; only warm (model held fixed) are they
  byte-comparable.
- A label scored before exp206 has no ledger: its diagnostics are `-`, not
  zero. The eval leaderboard folds percentiles as the worst pair, not a
  pooled percentile.
- No per-request JSONL ledger (the brief's optional item): the aggregate
  is written; per-request records stay the `-vv` log's.

### The category list (fixed, documented in `humanify-ag-model/src/call_ledger.rs`)

Every call is filed under its namer (`naming` = waves + sweeps, `modules`,
`vendor`, `split`, `treeReviser` — `providerErrors`' five) in three views:

- **outcomes** (byte-identical with the model held fixed — warm vs cold, any
  completion order; tested): `calls`, `answered`, `failed` (after every
  retry; equals `providerErrors` per namer, tested on a failing stub),
  `truncated` (`finish_reason: length`), and per ASKED identifier one raw
  verdict, in priority order: `missing` (absent/null/empty — a vendor
  decline or a reviser's "already good" too), `unchanged` (handed back as
  itself), `invalid` (not a legal target, by `is_valid_rename_target`),
  `duplicateInAnswer` (same name for two ids of one answer), `takenName`
  (a name the prompt listed as used), else `proposed`; beside them
  `hinted` / `equalsPriorHint` (the model repeating the prior's name) and
  `extraKeys`.
- **transport** (moves with the server and the cache): `servedMemo`,
  `servedDisk`, `sent`; a sent call is `okFirstTry`, `okAfterRetry`, or
  `failed.{timeout,connection,status,malformed,cacheMiss,other}`;
  `httpAttempts` (both retry layers) and `retried.{timeout,connection,
  status429,status5xx,statusOther,malformed}` per retried attempt; `tokens`.
- **timing** (wall clock): `callMs` (a sent call, retries and backoff
  included) and `attemptMs` (each HTTP attempt): n, p50, p90, p99, max, mean.

Backups the pipeline took after a rejected answer stay the namers' record,
in `reask`: existing `lateRejections`, `invalidSuggestionFinishes`
(sanitized), `allFailedWindows`, `sweepReasked/Applied/Dropped`,
`unrecoverableRejections`, plus (new) `collisionHandoffs`,
`laneEndHandoffs`, `ladderAtLaneEnd`, `ladderAtBarrier`; and
`promptGuard.retryAsked` (identifiers re-asked).

### Cards that carry FALSE ZEROS (coverage.llm)

Every stats file written by the Rust binary before this lane carries
`coverage.llm.retries: 0` and `avgResponseTimeMs: 0` and no token fields —
hard-coded in core `naming/driver.rs` `coverage_inputs` (the
`MetricsTracker` that should have fed them was never wired). In
`dev/eval/results/` that is 28 labels, every one scored since 2026-09-26:
rust-5b-c3b272f-a/b, rust-relaxed-default-843826be, control-843826be-8gpu,
candidate-8af0574f-8gpu, every `*-scratch` candidate/control label,
ref-scratch-0f338ffa-r1/r2/r3(-s2), ref-names-0357696e-r1/r2/r3(-s2); plus
ref-names-111401fe-r1/r2/r3 (on the frozen tree only). TS-era cards
(106 labels, up to session-2026-08-05 / wave-*) have real
`avgResponseTimeMs` and tokens; their `retries: 0` counted the limiter
layer only (the SDK layer's retries were never counted anywhere).

### Proposed CLAUDE.md text (not applied — lane rule)

Under "Validating cross-version changes", after the per-surface KPI section:

> **Why did a KPI move? Read the LLM call diagnostics (exp206).** Every run's
> `--stats-json` carries `llmCalls`: per namer, the call ledger's
> `outcomes` (answered / failed / raw answer verdicts per asked identifier —
> proposed, taken, duplicate, invalid, unchanged, missing, equals the
> prior's hint; byte-identical with the model held fixed), `transport`
> (served from memo/disk vs sent; first try / after retry with each retried
> attempt's reason; failed by kind; tokens) and `timing` (call and attempt
> latency p50/p90/p99/max). `eval summarize` prints it per pair and
> `eval leaderboard` side by side ("p90ms 2000 vs 6800 (+4800)"). They are
> DIAGNOSTICS: no band, no grade — they separate a slower or failing server
> from the model answering differently from real failures. Labels scored
> before exp206 print `-`; their `coverage.llm.retries` / `avgResponseTimeMs`
> are false zeros (Rust era) — never read them.
