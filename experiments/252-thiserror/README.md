# exp252 — error types derived with thiserror; no string errors left in the shell

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), NEUTRAL on two
pairs (warm, standing priors, 0 cache writes on both legs).
Branch `exp252-thiserror`, on origin/main `cfcd1c710` (exp251 merged), not
merged. Step C2 of the functional-core migration
(experiments/243-effects-ratchet/README.md, "Refined step order"), plus
the `thiserror` conversion of every product crate's error types.

## Totals

- **Product code: 99,560 lines before, 99,372 after (-188).** Non-blank,
  non-comment lines in the non-test `.rs` files of each product crate,
  counted the same way on `cfcd1c710` and on this branch:

  | crate | before | after | change |
  | --- | ---: | ---: | ---: |
  | humanify-ag-core | 79,141 | 78,404 | -737 |
  | humanify-ag-cli | 8,943 | 9,057 | +114 |
  | humanify-ag-index | 7,544 | 8,004 | +460 |
  | humanify-ag-io | 336 | 316 | -20 |
  | humanify-ag-llm | 1,516 | 1,516 | 0 |
  | humanify-ag-model | 2,080 | 2,075 | -5 |
  | **total** | **99,560** | **99,372** | **-188** |

  The core and io lose their hand-written `Display` / `Error` / `From`
  impls. The index and cli grow: their 56 string errors became typed enums,
  and a typed variant takes more lines than the one `format!` it replaces
  (the index spec's 25 validation messages are most of it). The net
  reduction is small, not large.
- **census:effects: 247 entries before, 191 after.** All 56 `legacy`
  entries are deleted: index 42, cli 14. Nothing was added. No
  `Result<_, String>` is left in a product crate's non-test code.
- **No error text changed.** Every variant of every error type in core
  and io, and every variant that formats data in index and cli, is pinned
  in an `error_text_pin_test` (one per crate); model's `LlmError` in
  `llm_test`. The core and io pins were written
  first and pass on the base code as well as on the derived code. Red seen
  by changing one word of one `#[error]` in each crate.
- **`thiserror` 2.0.21** (released 2026-09-23), one workspace dependency.
  Cargo.lock gains exactly two packages, `thiserror` and `thiserror-impl`;
  its proc-macro dependencies (`syn` 3.0.6, `quote`, `proc-macro2`) were
  already locked. It is on the pure crates' dependency lists (core, model)
  in `census::effects::rules`. The llm crate does not use it.

## What changed

- **About 65 error types** in core (about 50), io (`TreeError`,
  `DiffError`, `ScratchError`, `SubprocessError`), model (`LlmError`) and
  cli (`PriorLoadError`, `WriteError`) now derive `thiserror::Error`. A
  wrapping variant is `#[error(transparent)]`, with `#[from]` exactly where
  a hand-written `From` existed. A field named `source` is the error's
  source.
- **Step C2: index and cli.** Every `Result<_, String>` became a typed
  error, one per module or edge, in the exp247 style. index:
  `error::FileError` (shared), `MapError`, `Malformed`, `SlimError`,
  `RecipeRunError`, `ToolError`, `PackumentError`, `AsOfError`,
  `RegistryError`, `SpecError`, `ShardError`, `ReplayError`,
  `MergeError`, `NpmError`, `BuildError`, `CacheError`, `PlanError`,
  `ListError`, `CommandError`. cli: `VerbError` (match, discover and
  unpack verbs), `ValueError`, `TomlValueError`, `SwitchError`,
  `IndexDirError`, `IndexStaleError` / `StaleWhy`, `NodeFsError`,
  `SplitFailed`. Where they wrap a core or io error, they take it through
  `#[from]`.
- **Control flow no longer reads error text.**
  - npm's date-blocked install (`ETARGET` + `notarget`): npm's stderr is
    classified once into `NpmError::DateBlocked` or `NpmError::Failed`, and
    the retry reads the variant. A package counts as date-blocked when any
    one attempt was. The old check scanned the joined text of all
    attempts, so it could also fire when the two marker words came from
    different attempts.
  - the registry's unpublished tombstone: `PackumentError::Unpublished`;
    the `NOT_FOUND` marker constant is gone.
  - the run-time plan's `E404` / `404 Not Found` text check is deleted.
    It could not match: a 404 has come back as an absent packument since
    the curl fetch, and that package is still dropped downstream.
  - `planner::list_absence_outage` returns a bool; its message is
    `PlanError::Outage`.

## Gate

`cargo dev check` on the final product commit:

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW (advisory; no new twin from this lane) |
| census:concurrency | PASS |
| census:effects | PASS (191 entries) |
| census:slop | REVIEW (advisory; index erosion -1.9pp, io +2.4pp because its small `Display` fns left the denominator) |
| census:comments | PASS |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 fixture pairs, 88 launches equal to the golden) |

## Neutrality

`cargo dev eval neutrality cfcd1c710 <pair> --priors /work/neutrality-priors
--cache /work/neutrality-cache` on the committed candidate `160a94bfc`,
one at a time under `/work/neutrality.lock`:

| pair | differing files | differing lines | exit codes | cache writes (candidate / baseline) | verdict |
| --- | ---: | ---: | --- | --- | --- |
| 2.1.118 → 2.1.119 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |
| 2.1.197 → 2.1.198 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |

Logs: `/work/exp252-neutrality-118.log`, `/work/exp252-neutrality-197.log`.

## Open items

- `From<PriorMatchError> for String` (core) and `From<ScratchError> for
  String` (io) stay: core tests and the harness still use them.
- The run-time index's `JobAnswer::Failed` still holds a String (it is
  data passed between jobs, not a function's error).
- The harness crate (`humanify-ag-dev`) is not a product crate and keeps
  its string errors.
