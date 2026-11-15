# exp244 — model is pure: the clock and the working directory come from the shell

STATUS: BUILT (red seen for the new census check), GATED (`cargo dev check`, all
13 stages), NEUTRAL on both pairs (warm, baseline +0). Branch `exp244-model-pure`, not merged.
Step A of the effects-ratchet order (experiments/243-effects-ratchet).

## Totals

- **`humanify-ag-model` does no I/O any more, and the census now refuses
  any allowlist entry there.** Its 2 effect sites are gone: the clock
  (`time::iso_now`) and the working directory (`js::node_path_resolve`,
  through `std::path::absolute`). Model is on the new
  `effects::rules::STRICT_CRATES` list: an effect or a `Result<_, String>`
  in it has no role it may be listed under, so the census fails it until
  it moves to the shell.
- **Model's dropped-error tags: 5 tagged functions (6 call sites), all
  triaged.** 4 are gone and 1 now has a precise reason:

  | function | was | now |
  | --- | --- | --- |
  | `time::iso_now` | clock read; a clock before 1970 printed the epoch | removed. `time::iso_from_system_time(at)` takes the reading and prints a pre-1970 instant as JS does (floor to the millisecond) |
  | `js::node_path_resolve` | read the working directory; on failure it fell back to the path as given | takes `cwd` as a parameter: `cwd.join(p)` then the lexical normalization. No Result left |
  | `js::array_index` | `key.parse::<u64>().ok()?` after checking for at most 10 ASCII digits | a digit fold. Ten digits always fit a u64, so there is nothing to drop |
  | `CallLedger::outcome` | `result.as_ref().ok()` | a `match`. The error was never lost: the failed call is counted in the same function |
  | `js::numeric_literal_value` (2 sites) | untriaged | kept, tagged `// default:`. A spelling the radix or float parser refuses is "not a numeric literal", the function's documented `None`. One known gap is written in the tag: a radix literal above `u128::MAX` also reads `None` where JS has a float |

- **census:effects entries: 416 before, 417 after.** Model lost 2 `legacy`
  entries (model now has 0). The shell gained 3 `shell` inventory
  entries, the places that now read what model used to: the cli log's
  `timestamp` (clock), `RunReports::write_diagnostics` (clock) and
  `env::working_dir` (env). Core and the other crates have the same
  counts as before.
- **The working directory is read once, at the parse point.**
  `Config::from_process` calls the new `crate::env::working_dir()` (the
  env module is the only reader of process inputs, held by
  `tests/env_guard.rs`) and stores it in `config.io.working_dir`, which
  serde skips, so the recorded run config does not change. The split
  stage and the finish stage get it as `working_dir` on their inputs.
  Core's scaffold (`resolve_external_versions`, `write_runnable_scaffold`)
  takes it as a parameter.
- **One behaviour difference, in a case the pairs do not reach:** for an
  input given as a bare file name (`humanify-ag bundle.js`), its directory
  is the empty path. `std::path::absolute("")` is an error, so the old
  code walked `node_modules` from `/`. Now it walks from the working
  directory, which is what Node's `path.resolve("")` does (the TS
  original). Eval and e2e inputs are full paths. A working directory that
  cannot be read now fails the run at config time with a message instead
  of being silently skipped.

Raw diff lines against origin/main `3ea3a7baf`:

| | added | removed |
| --- | ---: | ---: |
| product code (model, core, cli, index) | 99 | 62 |
| product tests (model clock / path / array-index tests, scaffold call sites) | 66 | 4 |
| harness code (census strict crates, the runners' one `now_iso`) | 52 | 18 |
| harness tests (the strict-crate test) | 26 | 0 |
| allowlist and docs/responsibility.md | 21 | 15 |

## Gate

`cargo dev check` on the commit:

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS (model's clippy.toml bans are now met with one tagged allow left, on `numeric_literal_value`) |
| census:clones | REVIEW (advisory; 33 groups, the same count as main) |
| census:concurrency | PASS |
| census:effects | PASS (417 sites, 417 entries; model 0) |
| census:slop | PASS |
| census:comments | PASS |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 fixture pairs, 88 launches equal to the golden) |

Red before green: the new census test (`a_strict_crate_refuses_every_entry`)
failed on the old census (it printed a `legacy` entry to paste for a
model site); with the strict list, the repository test then failed on the
2 model entries until model was migrated. The model tests for
`iso_from_system_time` and the new `node_path_resolve` signature were
written first and did not compile against the old API. The
`array_index` test guards a refactor and passes on both.

## Neutrality

Candidate: commit `13b217fb8` (this branch's code commit). Baseline:
origin/main `3ea3a7baf`. Default priors (`/work/neutrality-priors`), warm
shared cache (`/work/neutrality-cache`), one run at a time under
`/work/neutrality.lock`.

| pair | differing files | differing lines | exit codes | cache writes (candidate / baseline) | verdict |
| --- | ---: | ---: | --- | --- | --- |
| 2.1.118 -> 2.1.119 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |
| 2.1.197 -> 2.1.198 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |

The load-bearing output tree (diagnostics-only artifacts excluded, as the
instrument reports) is byte-identical, including the scaffold's
installed-version lookup, which now resolves against the passed-in
working directory.

## Open items

- The rest of the 243 order: B (processes out of core), C (typed errors),
  D-I. Core is still in ratchet mode (65 legacy effect sites, 128
  string-error signatures).
- `numeric_literal_value` reads `None` for a radix literal above
  `u128::MAX`; JS gives a float. No bundle is known to spell one.
- index `registry::now_utc` still reads the clock with its own
  `duration_since(..).unwrap_or(0)`; it truncates to whole seconds on
  purpose, so it was left as it is (shell code).
