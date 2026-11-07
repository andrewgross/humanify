# exp199 — harness migration lane L4: the gate, the censuses, e2e and the dev tools in Rust

STATUS: DONE (2026-11-07), branch `harness-l4-gate`. Scope is
`docs/dev-harness.md` §5 lane L4 under Andrew's 2026-11-07 decisions
(keep the webcrack shim and node for running outputs; the old TS tools
stay until L5 deletes them). Built on L1 (exp195).

## Totals

- **14 verbs ported or added** in `crates/humanify-ag-dev`: `check`,
  `census concurrency`, `census clones`, `census slop` (new), `e2e`,
  `format-golden`, `no-private-addresses` (new stage), `shims` (new
  stage), `ask-trace`, `diff-asks`, `match-truth`, `env-check`,
  `precommit`, `stub-llm serve`. About 13,400 lines added, 1,700 removed,
  over 102 files.
- **`cargo dev check` is green, ALL 11 STAGES PASSED** (census:clones
  prints REVIEW: advisory, 37 findings, below). Every pass/fail stage
  was shown RED once with a planted failure.
- **e2e byte-equal to the TS e2e** on every fixture: the only differing
  files are the run's own `fresh.toml` / `fresh.stats.json`, and every
  differing line is an output path, the stub's port, or the TOML's sha
  (which follows from its paths). 152 launches equal the L0 golden.
- **Concurrency census verdict equal to the TS census**: byte-identical
  output on the repo, with an empty allowlist (103 / 103 sites), on all
  cargo registry sources (1,783 sites) and on all 16 planted trees.
- **match-truth scorecards byte-identical** to the TS runner's on the
  full 13-package corpus with the same binary (3,084 / 3,088 must-match
  pairs found in both).
- **One stub server, one fake**: the 7 hand-written stub servers and all
  28 in-order test fakes (the brief said 27; a two-line impl hid one) now
  use `humanify-ag-testkit`. No test expectation moved under the shuffled
  completion order (core + cli under four extra seeds): **no
  completion-order bug found**.
- **`cargo test --workspace`: 2,234 passed, 0 failed, 5 ignored** (inside
  the gate run).
- **Found and fixed on the way**: `node --check` passes an ES-module `.ts`
  file whatever it holds — the first `shims` stage was vacuous; the
  planted red caught it (§4).

## 1. Old path -> new verb (L5 deletes the left column)

| Old                                                                                   | New                                                                                                         |
| ------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `scripts/check.ts` (+ the `check`, `check:all`, `test` npm scripts)                   | `cargo dev check [--only a,b]` (`gate/check.rs`)                                                            |
| `scripts/concurrency-census.ts`, `test/concurrency-census.test.ts`                    | `cargo dev census concurrency [--files a.rs,...] [--root D] [--allowlist F]` (`census/concurrency*`)        |
| `scripts/clone-census.ts`, `test/clone-census.test.ts`, `test/fixtures/clone-census/` | `cargo dev census clones [--loose] [--root D]` over `crates/`; allowlist `scripts/clone-allowlist.json`     |
| (new)                                                                                 | `cargo dev census slop [--base REF] [--root D]` (§7 of the design; rule `slop/1`)                           |
| `scripts/e2e.ts`, `test/e2e-known-gaps.test.ts`                                       | `cargo dev e2e [--out D]` (`e2e/run.rs`, `expect.rs`, `launch_log.rs`)                                      |
| `scripts/format-golden.sh`                                                            | `cargo dev format-golden`                                                                                   |
| `test/no-private-addresses.test.ts`                                                   | `cargo dev no-private-addresses` (a gate stage)                                                             |
| the TS gate's typecheck / lint / knip / knip:prod                                     | `cargo dev shims` (the webcrack shim parses; nothing else of the TS remains after L5)                       |
| `scripts/ask-trace.ts`, `test/ask-trace-harness.test.ts`                              | `cargo dev ask-trace` (`--set` is now `--pipeline-set`, checked against the pipeline registry)              |
| `scripts/diff-asks.ts`                                                                | `cargo dev diff-asks`                                                                                       |
| `scripts/lib/stub-llm.ts`                                                             | the testkit stub in-process; `cargo dev stub-llm serve [--port N]` for by-hand runs                         |
| `experiments/lib/match-truth/*` (incl. `score.test.ts`, fixtures)                     | `cargo dev match-truth [--corpus-dir D] [--bin P] [--out D] [--only a,b]`; cache `/work/match-truth-corpus` |
| `scripts/rust-env-check.sh`                                                           | `cargo dev env-check [--endpoint URL]`                                                                      |
| `.husky/pre-commit`, lint-staged, `scripts/rustfmt-staged.sh`                         | `.githooks/pre-commit` -> `cargo dev precommit` (L5 sets `core.hooksPath`, see §5)                          |
| `crates/humanify-ag-llm/src/stub_server.rs` and 6 more hand-written stubs             | DELETED / ported onto `humanify_ag_testkit::stub_server` (done in this lane)                                |
| `test/measurement-owners.test.ts` rules 9, 10                                         | `crates/humanify-ag-dev/tests/measurement_owners.rs` (rules 7, 8 were L1 tests already; table §6)           |

## 2. The gate

`STAGES` (cheapest first): shims, no-private-addresses, rust:fmt,
rust:clippy, census:clones (advisory), census:concurrency, census:slop
(advisory), rust:unit, rust:build, format-golden, e2e. Each stage streams
its output (`proc::run_streamed` over the new
`humanify_ag_core::subprocess::run_inherited`: nothing piped, one deadline,
group kill), fail-fast, advisory REVIEW never FAIL, `--only` labelled
PARTIAL. `SUCCESSORS` maps all 13 `check.ts` stages; a committed test holds
the mapping complete and (until L5) equal to `check.ts`'s own list.

| check.ts stage                   | Successor                                        |
| -------------------------------- | ------------------------------------------------ |
| typecheck, lint, knip, knip:prod | `shims`                                          |
| rust:fmt, rust:clippy            | unchanged                                        |
| census:clones (advisory)         | `census:clones` over `crates/` (advisory)        |
| census:concurrency               | `census:concurrency`                             |
| unit                             | MOOT as TS: ported into rust:unit                |
| rust:unit, rust:build            | unchanged                                        |
| rust:format-golden               | `format-golden`                                  |
| e2e                              | `e2e`                                            |
| (new)                            | `no-private-addresses`, `census:slop` (advisory) |

## 3. Proof

| Proof                                                                              | Result                                                                                                                                                                                          |
| ---------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo dev check` in the worktree (`a3fec5fc7`)                                    | ALL 11 STAGES PASSED (rust:unit 286 s, rust:build 127 s, e2e 79 s); census:clones REVIEW                                                                                                        |
| `cargo dev check` from a clean checkout                                            | see §8                                                                                                                                                                                          |
| Every pass/fail stage RED once (planted, `--only <stage>`, scratch worktree)       | shims, no-private-addresses, rust:fmt, rust:clippy, census:concurrency, rust:unit, rust:build, format-golden, e2e (golden and expectation): all exit 1, FAIL named; each unplanted rerun exit 0 |
| Gate logic tests (fail-fast, not-reached, REVIEW, PARTIAL, unknown stage, mapping) | 8 pass; red seen (mapping order; fail-fast and REVIEW broken on purpose: 2 fail)                                                                                                                |
| e2e vs TS e2e, kept trees, `diff -r`                                               | only `fresh.toml` / `fresh.stats.json` differ (16 pairs), every line a path, the stub port or the TOML sha                                                                                      |
| e2e launch log vs `launch-goldens/e2e.golden.txt`; ask-trace vs its golden         | byte-identical (committed tests); 152 launches                                                                                                                                                  |
| Concurrency census vs TS                                                           | identical exit and output: repo, empty allowlist (103 sites), registry sources (1,783 sites), 16/16 planted trees                                                                               |
| match-truth vs TS runner, 13 packages, same binary                                 | scorecards byte-identical; launches equal the L0 golden                                                                                                                                         |
| format-golden vs `scripts/format-golden.sh`                                        | same output (302 legs identical; plant detected)                                                                                                                                                |
| env-check vs `scripts/rust-env-check.sh --llm`                                     | same verdicts line by line; RUST ENV OK                                                                                                                                                         |
| no-private-addresses                                                               | TS vectors ported; red seen for real: my own first draft carried an address in a comment                                                                                                        |
| `cargo test --workspace` after the stub/fake consolidation                         | 2,098 / 0 / 5 on the sub-lane head; 2,234 / 0 / 5 in the merged gate run; no test lost                                                                                                          |
| Shuffled fake: core + cli lib tests under 4 extra seeds                            | every expectation unchanged                                                                                                                                                                     |
| `exp162_anchor` with its variable unset                                            | passed green before; now panics with its usage line (2 `should_panic` tests)                                                                                                                    |
| `launch_parity_test` reads the goldens (143 launches)                              | a planted `concurrency = 471` in a golden fails it, naming the key                                                                                                                              |
| `npm run check` (worktree, clean checkout)                                         | see §8                                                                                                                                                                                          |

## 4. What changed from the design, and why

1. **`shims` does not use `node --check`.** On an ES-module `.ts` file
   it exits 0 whatever the file holds, so the stage as first written was
   vacuous; the planted red caught it. Each shim is now type-stripped
   (`module.stripTypeScriptTypes`) and compiled as a `vm.SourceTextModule`.
   No prettier on shims: prettier is an npm tool, and the only shim is the
   webcrack one.
2. **Flags are harness keys** (one parse point): a new `[dev]` section
   (`--only`, `--files`, `--loose`, `--root`, `--allowlist`, `--base`,
   `--prior`, `--collide`, `--sequential`, `--port`, `--corpus-dir`).
   `--out` is ONE key, `run.out`, shared with L2b's library rescore: both
   lanes had declared `--out` (`dev.out`, `libscore.out`), which clap
   refused once main was merged in — one flag, one key. match-truth's `--corpus <dir>` became `--corpus-dir`
   (`--corpus` is the library scorer's); ask-trace's `--set` became
   `--pipeline-set`. The census takes `--files a,b` only (no positional
   files).
3. **Verbs receive the host environment from main** (`run(parsed, host)`,
   `HostEnv` re-exported at the crate root): env_guard forbids any
   `crate::env::` use outside env.rs / main.rs. `HostEnv` gained
   `current_exe` (the gate re-runs this binary for in-crate stages) and
   `cwd`.
4. **`subprocess::run_inherited`** in core (product crate, harness use only;
   3 allowlist entries) and `proc::run_streamed`; proc's one
   `Command::new` moved into `proc::command` (entry re-keyed).
5. **The clone census finds near-copies too**: an identical-stream census
   cannot see the `matching.rs` fingerprint twin, so two functions sharing
   an unbroken 80-token run are a finding (80 measured: 100 drops the twin,
   60 doubles the list). Same-file findings are reviewed; test-only ones are
   listed, never fail. The allowlist starts EMPTY, so the stage prints REVIEW
   with **37 findings** (2 identical groups, 35 shared runs: the fingerprint
   twin, a date helper copied 4 times, `inverted_callers` twice,
   `is_babel_statement` 3 times, ...). Unifying them is product work for
   their own lanes; allowlisting them blind would defeat the census.
6. **`census slop`** is exactly §7, plus its review rule: exit 1 (REVIEW)
   when a crate's non-test erosion or clone share rises more than 0.5
   percentage points against the merge-base, or a new
   `cognitive_complexity` allow appears.
7. **match-truth canonical form**: the arrow-flip check uses oxc and the
   pipeline's own rule (one new additive pub fn in core
   `hash::wrapper_spelling`, nothing in the pipeline calls it). The one
   verdict that differs from the TS is a `new.target` inside a nested
   ordinary function (TS refused, Rust allows: it is the nested function's
   own); it never occurs in the corpus, and a test pins it.
8. **launch_parity_test stays in the cli crate** (it tests that crate's
   private parse code; a product crate must not depend on the dev crate)
   and reads the goldens with a small parser of its own. Four
   combinations with no commander-era capture (e2e's unsplit
   `--sequential` legs, ask-trace's split leg and batch size 10) are
   declared in `NEW_COMBINATIONS` with reasons.
9. **One test expectation changed**, explained: llm `client_test`'s usage
   numbers are now the testkit stub's canned block (2/1/1) instead of the
   deleted twin's (15/10/5). The client is unchanged.
10. **The precommit hook is committed, not installed.** Switching
    `core.hooksPath` is shared git config: it would turn off husky's
    prettier for every lane while the TS gate's lint stage still runs. L5
    sets it when husky goes.

## 5. Proposed CLAUDE.md text (for L5; this lane does not edit CLAUDE.md)

Replace the "Checks" section's command with:

> **One command. Run it before every commit:** `cargo dev check`. It runs
> eleven stages — shims, no-private-addresses, rust:fmt, rust:clippy,
> census:clones (ADVISORY), census:concurrency, census:slop (ADVISORY),
> rust:unit, rust:build, format-golden, e2e — and prints which ran.
> `--only a,b` is labelled PARTIAL. The stage list is `STAGES` in
> `crates/humanify-ag-dev/src/gate/check.rs`; nothing outside it runs.
> Install the hook once per clone: `git config core.hooksPath .githooks`.

and the individual-stage list with the `cargo dev <verb>` spellings of §1.

## 6. measurement-owners.test.ts: the 15 rules

| #   | Rule                                                   | Fate                                                                                  |
| --- | ------------------------------------------------------ | ------------------------------------------------------------------------------------- |
| 1   | changed-line counting stays in the declared sites      | **hand to L2a**: its counting module is the one place; a test fails any other counter |
| 2   | statement extraction routes through the throwing owner | **hand to L2a**: extraction errors when wrapper detection fails, never falls back     |
| 3   | recorded verdicts are consumed by the summary          | **hand to L2a** (summarize): boot and self-hop verdicts read and shown                |
| 4   | every verdict file the run writes is read              | **hand to L3 + L2a**: the list derived from the writer's constants, each one read     |
| 5   | leaderboard refuses bands from another commit          | **hand to L2a**: through the identity guard, `--force-mixed` the only override        |
| 6   | the TS pipeline is gone                                | moot                                                                                  |
| 7   | every verb declares proves and cannotProve             | L1 `main_test`                                                                        |
| 8   | an unknown flag fails upfront                          | L1 `settings_test`, `main_test`                                                       |
| 9   | binary cards never join a TS label                     | ported: `tests/measurement_owners.rs`                                                 |
| 10  | the `--bin` family is flags, retired switches fail     | ported: `tests/measurement_owners.rs`                                                 |
| 11  | the TS stage-6 adapter is gone                         | moot                                                                                  |
| 12  | no ambient eval env vars                               | structural: `env_guard.rs`, `HostEnv` carries only home, path, held_lock, exe, cwd    |
| 13  | no `VAR:-default` in shell scripts                     | moot with the scripts                                                                 |
| 14  | run.sh / neutrality.sh read ONE run config             | moot: typed run config, one parse point                                               |
| 15  | neutrality.sh refuses flags and positionals            | moot: `settings::parse`                                                               |

## 7. What I could not prove

- The match-truth canonical cases that could differ from Babel in theory
  (script-then-module parse vs `unambiguous`; child order by source
  position vs visitor keys; a preview cut inside a surrogate pair). None
  occurred on the corpus.
- The concurrency census on construct spellings that appear in neither
  the repo nor the registry sources (covered by the line-for-line port
  only).
- `census slop`'s thresholds are a first choice (0.5 pp), not measured
  against a history of real changes.
- The precommit hook as the INSTALLED hook (not installed, §4.10). Run by
  hand in a scratch worktree it did each job: a misformatted staged `.rs`
  was formatted and re-staged (exit 0); a staged unlisted `thread::spawn`
  turned the census RED; a staged private address was refused. Note that
  `cargo dev` rebuilds the harness when product crates changed (26 s
  then), so the hook is slower than lint-staged was.

## 8. Gate

Filled in below at merge time.
