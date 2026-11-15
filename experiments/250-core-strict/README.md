# exp250 — strict core: no effect site and no string error left in core

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), NEUTRAL on two
pairs (warm, standing priors, 0 cache writes on both legs), artifact dump
byte-identical. Branch `exp250-core-strict`, not merged. Steps G and I of
the functional-core migration (experiments/243-effects-ratchet/README.md,
"Refined step order").

## Totals

- **Core has no `legacy` entry left, and the census now refuses one.**
  `humanify-ag-core` had 22 `legacy` entries in
  `dev/census/effects-allowlist.json` at origin/main `86572fc80`; it has 0.
  Its only entries are the 4 `observation` entries of the profiling clock.
- **census:effects: 264 entries before, 247 after.** 22 core `legacy`
  entries deleted, 5 cli `shell` entries added (the dump folder and the
  ask log writer in `cli/src/writers.rs`, and `Disk::has_file`).
- **Both pure crates are strict.** `effects::rules::STRICT_CRATES` is
  deleted: every crate in `PURE_CRATES` refuses every role, except
  `observation` in the profiling files. `legacy` now means only one thing:
  a `Result<_, String>` in a shell crate (cli, index).
- **No behaviour change on the pairs**, and the `--dump-artifacts` folder
  (302 MB, 15 files, 3,190 prompt rows) and the `--dump-asks` log are
  byte-identical to origin/main's.

What moved where:

| was in core | now |
| --- | --- |
| the input file read (`unpack::input::read_input`) and the chunk linker's reads (`chunk_link`: the sibling check, each chunk, the unreached `.js` count, the embedded files) | through the shell's `output_tree::FolderReader`, the reader the output tree already uses for the files beside the input; one method added, `has_file` (the cli's `Disk` asks `Path::is_file`; the default reads the file) |
| the dump's section files and the streamed prompts.jsonl part files (`artifact_dump::DumpSink`, `LogFiles`) | core keeps the file names, the bytes, the part order and the assembly; the files are created, read back and removed through `artifact_dump::DumpFolder`, which the cli implements (`writers::DumpDir`). The dispatch log still streams each row to its part file at commit |
| the `--dump-asks` writer (`write_ask_rows`) | core renders the text (`artifact_dump::ask_log`), the cli writes it (`writers::write_ask_log`) |
| `Result<_, String>` in `prior.rs` (5) and `PriorManifest::parse` | `prior::PriorMatchError` / `SideParseError` (hand-written `Display`, the old texts, pinned in `error_text_pin_test`); `PriorManifest::parse` returns `serde_json::Error`. The match functions are generic over the consumer's error (`E: From<PriorMatchError>`); the match verb's string errors convert through one `From<PriorMatchError> for String` |

No `// io:` tag is left in core's non-test code outside profiling.

Raw diff lines against origin/main `86572fc80` (comments included):

| | added | removed |
| --- | ---: | ---: |
| product code (core, cli, model) | 378 | 229 |
| tests | 210 | 60 |
| harness (census rules, e2e runner call) | 28 | 34 |
| census allowlists (effects, clones) | 34 | 122 |
| docs (responsibility, pipeline-stages) | 7 | 5 |

The product code grew by 149 lines. The typed prior errors are about 70
of them (an enum, a struct, their `Display`), the dump folder trait and
the cli's disk side about 60. What went: the core's own file writes and
reads in the dump and the input reader, and the strict-crate list.

## Behaviour notes (none reachable on the pairs)

- The unreached-`.js` count in the link report: a folder listing that
  fails on one entry now counts 0; before, the failing entry was skipped.
- `is_bun_module` reads the whole `.js` file a path string names instead
  of its first 7 bytes (same answer; only for path strings naming `.js`
  files).
- A dump part file is flushed before it is removed on a failed run's
  discard (before: removed, then flushed into the unlinked file). No file
  is left either way.

## Gate

`cargo dev check` on the candidate (and again on the final commit):

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW (advisory; this lane's two findings, the core test dump folder twinning the cli's `DumpDir::create` / `open`, allowlisted with the same reason as exp249's `Disk::list`) |
| census:concurrency | PASS |
| census:effects | PASS (247 entries; core: 4 observation, 0 legacy) |
| census:slop | PASS |
| census:comments | PASS (no new findings) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS |

Red seen before green: `error_text_pin_test::prior_match_errors_keep_their_words`
(the error types did not exist), `input_test::the_input_and_its_chunks_are_read_through_the_folder_reader`
(an in-memory reader, no file on disk), the dump tests against the new
`DumpFolder` signatures, and three census tests
(`a_pure_crate_refuses_every_entry` now over core and model,
`an_unlisted_effect_in_a_pure_crate_fails_naming_the_shell`,
`the_profiling_clock_is_an_observation_only`) failed before the rules
change.

## Neutrality

`cargo dev eval neutrality 86572fc80 <pair> --priors /work/neutrality-priors
--cache /work/neutrality-cache` on the committed candidate `23d4a7989`,
one at a time under `/work/neutrality.lock`:

| pair | differing files | differing lines | exit codes | cache writes (candidate / baseline) | verdict |
| --- | ---: | ---: | --- | --- | --- |
| 2.1.118 → 2.1.119 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |
| 2.1.197 → 2.1.198 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |

Logs: `/work/exp250-neutrality-118.log`, `/work/exp250-neutrality-197.log`.

## Artifact dump byte identity

Both binaries (origin/main `86572fc80` built by `cargo dev build-bin`, and
the candidate) run by hand on 2.1.118 → 2.1.119 with the neutrality leg's
config and flags plus `--diagnostics-dump-artifacts` and
`--diagnostics-dump-asks`, warm cache (the cache's file count was 143,777
before and after both runs), one at a time under the neutrality lock:

| output | result |
| --- | --- |
| dump folder (15 files, 302 MB; prompts.jsonl 3,190 rows; no part file left) | `diff -r`: identical |
| ask log (3,190 rows) | `cmp`: identical |
| output tree | `diff -r`: identical |
| exit codes | 0 / 0 |

Folder: `/work/exp250-dump/`. The `dump_coverage` and `ask_trace` cli
tests and the core streaming test (`fast_test`) pass.

## Open items

- `placement_dump` (step G's other half in the 243 plan) has no effect
  entry left: its reads went with the prior tree and its writes with the
  output tree.
- Step H (the `// default: legacy, untriaged` dropped-error tags) is not
  touched here.
- The webcrack shim still writes into the output folder itself (exp249's
  open item).
