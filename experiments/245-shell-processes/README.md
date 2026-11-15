# exp245 — processes out of the core: a small shell I/O crate, `Differ`, and the shim run by the shell

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), NEUTRAL on two
pairs (warm, standing priors, 0 cache writes on both legs). Branch `exp245-shell-processes`, not merged. Step B of the
functional-core migration (experiments/243-effects-ratchet/README.md,
"Refined step order").

## Totals

- **The core no longer starts a process, makes a temp dir or reads the
  environment for one.** `subprocess.rs` and `scratch.rs` moved out of
  `humanify-ag-core` into a new shell crate, `humanify-ag-io`, which the
  core does not depend on (only as a dev-dependency, for its tests). The
  system `diff` moved with them (`humanify_ag_io::diff`), and the core takes
  it through a `Differ` trait. The webcrack shim is run by the cli
  (`humanify_ag_cli::webcrack::WebcrackShim`); the core gets what it printed
  through an `ExternalUnpacker` trait and keeps the decisions (the contract
  line, files outside the out dir refused).
- **census:effects: 417 entries before, 415 after** (origin/main
  `d69a2615a`). 15 `legacy` entries deleted (core `legacy` entries
  193 → 178; all `legacy` 249 → 234), 13
  `shell` entries added for the same sites in their new homes (io and cli).
  `legacy` is never added.
- **census:concurrency: 124 entries before and after.** The 12 moved sites
  were re-keyed (file, and the webcrack site's scope), not re-listed; the
  reason texts now name `humanify_ag_io::subprocess::run`.
- **Product behaviour is unchanged by construction**: the same `diff`
  argv, the same temp-file normalisation, the same shim argv, cwd and
  deadline, and every error text word for word. Neutrality proves it on
  both pairs: 0 differing files, 0 differing lines, 0 / 0 cache writes.

Raw diff lines against origin/main `d69a2615a` (comments included):

| | added | removed |
| --- | ---: | ---: |
| product code (core, cli, index, io; Cargo.toml) | 379 | 198 |
| tests | 373 | 189 |
| census allowlists | 104 | 116 |
| docs (concurrency.md, responsibility.md) | 18 | 15 |

Most of the code additions are moves across files (git counts a function
moved to a new file as added there and removed here): `io/diff.rs` (67) is
`hunks.rs`'s `compute_normal_diff`, `cli/webcrack.rs` (81) is
`unpack/webcrack.rs`'s process half. What is really new: the `humanify-ag-io`
crate's manifest and root (28), two typed errors (`ScratchError`,
`DiffError` in io; `DiffError`, `UnpackerError` in core: about 45 lines,
because a moved `Result<_, String>` would have needed a new `legacy` entry),
the two traits (about 30), and two small argument structs
(`StepRules`, `PostSplitRules`) that keep the reconcile entry points under
the 7-argument clippy ceiling once the differ joined them. Tests: the
46 `run_naming` / `name_vendor_bodies` / reconcile call sites in core tests
gained one argument each (the differ), which is most of the test churn.

## What changed

| site | before | after |
| --- | --- | --- |
| child-process runner | `humanify_ag_core::subprocess` | `humanify_ag_io::subprocess` (code unchanged; its test moved with it) |
| scratch dirs | `humanify_ag_core::scratch::temp_dir -> Result<_, String>` | `humanify_ag_io::scratch::temp_dir -> Result<_, ScratchError>` (same text through `Display`; `From<ScratchError> for String` so `?` callers are unchanged) |
| GNU diff | `core::naming::reconcile::hunks::compute_normal_diff`, called directly by the pre-split step and the post-split pass | `humanify_ag_io::diff::compute_normal_diff`; core declares `hunks::Differ` (blanket impl for any `Fn(&str, &str) -> Result<String, E: Display>`), and the shell passes it to `run_naming`, `name_vendor_bodies` and `FinishInput.differ` |
| post-split differ seam | private `post_split_reconcile_with(input, &dyn Fn)` for tests | `PostSplitInput.differ`, the one entry point |
| webcrack shim | core made the out dir, spawned `npx tsx`, judged the exit, parsed, checked paths | cli `WebcrackShim` (`impl ExternalUnpacker`) makes and canonicalizes the out dir, spawns, judges the exit and returns `ShimRun { out_dir, stdout }`; core `unpack_webcrack` parses the contract line and refuses a file outside `out_dir` |
| users of the runner | index (7 calls), dev (`proc`, `difftool`), cli `discover` | the same calls, now through `humanify_ag_io` |

The effects census now lists `crates/humanify-ag-io/` as a product crate
(it is linked into the release binary), so its effect sites are inventory
(`shell`). Its `Result<_, String>`-free API is why the move adds no
`legacy` entry.

Tests: the webcrack pipe/deadline/relative-out-dir tests moved to
`cli/src/webcrack/webcrack_test.rs` and drive real children; core's
`unpack/webcrack/webcrack_test.rs` is new and pure (a fake shim run:
order kept, outside path refused, a failed run's text kept word for word,
empty output refused). New: `io/src/diff/diff_test.rs` (normal format,
identical texts, CRLF, the deadline) and, in core, a pre-split step test
that a failing `Differ` is the step's error word for word (red before the
seam existed: it did not compile).

## Gate

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones (advisory) | REVIEW: 5 core TEST functions newly over the 80-token twin threshold, each beside a partner that was already listed (the shared `NamingInput` / `PostSplitInput` literal blocks grew by the differ argument; shortened to a `gnu_diff` import, which took it from 9 to 5). No product code in a new group. Left: unifying those test setups is outside this step |
| census:concurrency | PASS (124 entries) |
| census:effects | PASS (415 entries) |
| census:comments (advisory) | GREEN (no new finding) |
| census:slop (advisory) | REVIEW: `humanify-ag-io` erosion 0 → 55.2%: the moved `subprocess::run` (CC > 10) in an 11-function crate, not new complexity; core and cli erosion -0.1pp each |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS |

## Neutrality

`flock /work/neutrality.lock cargo dev eval neutrality d69a2615a <pair>
--priors /work/neutrality-priors --cache /work/neutrality-cache --workdir
/work/exp245-neutrality-<v>`. The candidate commit's product code is the
same as the committed branch head (only this README changed after).

| pair | cache writes candidate / baseline | exits | differing files / lines | verdict |
| --- | ---: | --- | ---: | --- |
| 2.1.118 -> 2.1.119 | 0 / **0** | 0 / 0 | **0 / 0** | **NEUTRAL** |
| 2.1.197 -> 2.1.198 | 0 / **0** | 0 / 0 | **0 / 0** | **NEUTRAL** |

(An earlier 2.1.118 run against the pre-rebase base `3ea3a7baf` was also
NEUTRAL, 0 / 0.)

## Open items

- `docs/responsibility.md` has one new row (diffing two texts the way the
  reconcile aligns them) and three re-pointed rows (child process, scratch
  dir, the webcrack shim's directory and trust check).
- Later steps (C typed errors, D `PriorTree`, F in-memory output) remove
  the rest of core's disk reads and writes; this step touched only
  processes, scratch dirs and the shim.
