# exp247 — typed errors in the core

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), NEUTRAL on two
pairs (warm, 0 / 0 cache writes); rebased onto origin/main `aa665e91f`.
Branch `exp247-core-errors`, not merged. Step C,
core half, of the functional-core migration
(experiments/243-effects-ratchet/README.md, "Refined step order").

## Totals

- **The core's functions no longer return `Result<_, String>`, except six
  around the prior release.** census:effects listed 125 `result-string`
  entries in `humanify-ag-core`; this lane removed 117. Of the 8 it left to
  the PriorTree step (exp248, which ran at the same time), exp248 removed 2
  (`load_prior_vendor`, `read_ledger`); 6 remain with exp248's signatures:
  `prior.rs` (`match_prior_version`, `match_stage_with_prior`,
  `with_prior_match_side`, `parse_side`,
  `assert_prior_looks_like_same_program`) and
  `modules::vendor_names::PriorManifest::parse`.
- **census:effects: 399 entries before, 282 after** (origin/main
  `aa665e91f`, exp248 merged; rebased). 117 `legacy` `result-string`
  entries deleted; nothing added. Remaining `result-string`: core 6, index
  42, cli 14 (index and cli are the next halves of step C).
- **Every private `Result<_, String>` in the core went too** (the census
  counts only non-private functions): the formatter's `R<T>` aliases, the
  runnable emit's planner, the chunk linker's passes, the reconcile step's
  and the deferred sweep's `(String, trail)` errors. A `grep` for
  `Result<…, String>` in the core's non-test sources now finds only
  `prior.rs` and `PriorManifest::parse`.
- **Each module or edge has its own error type, and each prints exactly
  the text the site printed before.** About 50 types: one per stage
  edge (`FormatError`, `CarryError`, `UsingError`, `FinishError`,
  `InventoryError`, `SplitInputError`, `RederiveError`, `FossilSplitError`,
  `PlaceError`, `ReconstructError`, `SplitError`, `EmitError`,
  `TransferError`, `EraError`, `NamingError`, `StepError`, `PermuteError`,
  `LinkError`, `InputError`, `WebcrackError`, `BunError`, `UnpackError`,
  `TableError`, `ManifestError`, `IndexError`, `AnswersError`,
  `RecipeError`, `DumpError`, `DetectError`, …), plus a shared
  `crate::error::FileError` (operation + path + the system error) for the
  core's remaining legacy disk sites. No new dependency: plain enums with a
  hand-written `Display`, as exp245's `ScratchError` / `DiffError` /
  `UnpackerError`; `thiserror` is not on the core's allowed list and is not
  needed.
- **Error text is output, so it is pinned.** `error_text_pin_test` builds
  a sample of every error type (every variant that formats data) and holds
  its `Display` to the format string the site used before, filled with the
  same values. Red was seen by changing one word of one `Display`.
- **The shell converts at its edge.** cli, index and dev call
  `.map_err(|e| e.to_string())` (or `Crash(e.to_string())`) where they
  still hold `String` errors; the text they print is the same bytes.
- **Lines: the code grows.** Typed errors spell each message once in a
  `Display` arm and give each variant fields and a doc line, where a
  string site was one `format!`. See the table.

Raw diff lines against origin/main `aa665e91f` (comments included):

| | added | removed |
| --- | ---: | ---: |
| product code (core, cli, index, dev) | 2,982 | 915 |
| tests (incl. the new pin test) | 611 | 79 |
| `dev/census/effects-allowlist.json` | 38 | 740 |
| docs (responsibility.md, this README) | 142 (incl. this README) | 0 |

## Gate

`cargo dev check` on the rebased commit: ALL 13 STAGES PASSED (also
before the rebase).

| stage | result |
| --- | --- |
| shims, no-private-addresses, rust:fmt, rust:clippy | PASS |
| census:clones | REVIEW (advisory; no new twin from this lane) |
| census:concurrency | PASS |
| census:effects | PASS (282 sites, 282 entries) |
| census:slop | PASS |
| census:comments | REVIEW (advisory; 0 new findings after two comment rewrites) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 fixture pairs, 88 launches equal to the golden) |

The allowlist JSON diff shows 38 "added" lines: git's alignment of the
remaining entries, not new entries (399 → 282, nothing added).

## Neutrality

Run on the pre-rebase commit `ce2dbea6c` against origin/main
`a78ab2ef3`, warm, `--priors /work/neutrality-priors --cache
/work/neutrality-cache`, under the shared lock:

| pair | differing files | differing lines | exit codes | cache writes (candidate / baseline) | verdict |
| --- | ---: | ---: | --- | --- | --- |
| 2.1.118→2.1.119 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |
| 2.1.197→2.1.198 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |

## Design notes

- **One type per module or edge, not one giant enum.** A stage's error
  wraps the errors of the stages it calls (`SplitError::Place(PlaceError)`,
  `PlaceError::Fossil(FossilSplitError)`, `FossilSplitError::Map(
  FossilMapError)`), each wrapper's `Display` passing the inner text
  through unchanged, so a message keeps the words it had however deep it
  was raised.
- **Text held as text only when it comes from outside.** A parser's first
  diagnostic, the shell's diff or shim message, and the prior-loading
  functions' messages (`NamingError::Prior`, `EraError::Parse`) stay
  `String` inside a typed variant. Fixed
  messages are `&'static str` in a named variant (`Refused`, `Internal`,
  `Unsupported`, `Traverse`).
- **The prior boundary.** `prior::match_prior_version` still takes a
  closure returning `Result<T, String>`. The naming era rides inside its
  `Ok` (`|stage| Ok(prior_era(..))`), so the era's typed error is not
  flattened to text; the call is the closure's last act in
  `run_match_stage`, so nothing runs differently. Two body lines in
  `prior.rs` turn a now-typed callee error into its text for the
  still-`String` functions (`.map_err(|e| e.to_string())`); no `prior.rs`
  signature changed.
- **Errors stored for later keep their type where they can.**
  `SplitOutcome::declined` is `Option<EmitError>`,
  `NamingOutcome::reconcile_error` is `Option<StepError>`,
  `VendorNaming::prior_refusal` is `Option<NamingError>`. Two stay text
  because their owners print them as part of a larger line:
  `PriorHashes::Refused` (the ledger log line) and
  `InputSplitMethod::NotSplit`'s record.
- **`LoadError::Unusable` now holds an `IndexError`** (which file, and
  whether it failed to open, to parse as JSON, its manifest stamps or its
  table); the old `From<String>` is gone.
- **`lookup_files_into` / `names_lookup_files_into` are generic over the
  writer's error** and return `TableWriteError<E>` (a table failed, or the
  writer refused its bytes); the in-memory forms use `Infallible`.
- **One size fix.** The reconcile step's error rides beside the strategy
  trail; boxing its diff variant keeps the pair under clippy's
  `result_large_err` limit.

## Open items

- index (42) and cli (14) `result-string` entries: the next halves of
  step C. The index's text-matched control flow (`ETARGET`, `E404`,
  `NOT_FOUND`) belongs there.
- The 6 core entries left (`prior.rs`, `PriorManifest::parse`): a small
  follow-up now that exp248 has merged (the prior match closure's
  `Result<T, String>` would become generic over the consumer's error).
- Rebase onto `aa665e91f` (exp248): conflicts in `emit/stable_split.rs`,
  `place/placement_dump.rs` (exp248 deleted `read_carry` / `read_json` and
  the fossil-modules write, so `PlaceError` lost its `PriorCarry` and
  `WriteFossilModules` variants), cli `unified.rs` and the allowlist; all
  were conflict resolution only, so neutrality was not re-run on the
  rebased commit.
