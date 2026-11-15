# exp249 — the output tree built in memory, written once

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), NEUTRAL on two
pairs (warm, standing priors, 0 cache writes on both legs). Branch
`exp249-memory-tree`, not merged. Step F of the functional-core migration
(experiments/243-effects-ratchet/README.md, "Refined step order").

## Totals

- **The core writes no file.** Before, the unpack wrote the tree to the
  output folder in the middle of the run, the naming loop read each
  unpacked file back from disk, the vendor-body naming read and rewrote
  vendor files, the split wrote its tree and metadata, and the finish
  read every file back and rewrote it in place. Now every stage puts its
  files into one value, `core::output_tree::OutputTree` (path → bytes, the
  paths the run removed, and the reader of what is on disk), and the cli
  writes it ONCE, after the finish
  (`humanify_ag_cli::output_tree::build_then_write` → `write_tree`).
- **census:effects: 282 entries before, 264 after** (origin/main
  `1e1c69628`). 21 entries deleted: 17 core `legacy` disk sites
  (finish/driver ×6: `finish_split_output`, `leftover_vendor_files`,
  `read_opt`, `relink_bun_modules`, `write_file`; library_names ×1;
  scaffold ×5: `installed_version`, `read_utf8`, `walk`,
  `write_embedded_files`, `write_runnable_scaffold`; using ×1;
  vendor_bodies ×2: `bodies_of_bridge`, `write_vendor_naming`; unpack ×1:
  `write_passthrough`; bun ×2: `unpack_bun`, `write_vendor_file`) and 4
  cli `shell` sites (`index_files::read_utf8`, split_stage
  `remove_consumed_source_file` and `write_file`, unified
  `NamingRun::run`). 3 `shell` entries added, all in the one writer
  (cli `output_tree.rs`: `Disk::read`, `Disk::list`, `write_tree`). No
  `legacy` entry added. Core `legacy` disk sites outside profiling: 33 → 16
  (what is left: the input reader and chunk linker, the artifact dump —
  step G).
- **Peak memory: unchanged within noise.** 2.1.197 → 2.1.198, warm cache,
  two runs each: 16,031 / 16,007 MB on origin/main, 16,059 / 16,141 MB
  here (+0.2% to +0.8%; the written tree is 113 MB on disk).
- **No new `Result<_, String>`.** The writer's error is typed
  (`output_tree::WriteError`, `Display` printing the old texts:
  `mkdir <path>: …`, `write <path>: …`, `copy the embedded file <from> to
  <to>: …`).

Raw diff lines against origin/main `1e1c69628` (comments included):

| | added | removed |
| --- | ---: | ---: |
| product code (core, cli) | 974 | 599 |
| tests | 514 | 68 |
| census allowlists (effects, clones) | 25 | 126 |
| docs (responsibility, pipeline-stages) | 9 | 5 |

The product code grew: the tree type and its overlay (core, about 240
lines with docs), the one writer and disk reader (cli, about 150) and the
explicit inputs/outputs of the stages that build the tree (cli
`build_tree`, its two structs) are new. What went: every write helper in
the finish, the unpack's three writes, the split stage's writes and its
"failed before the commit" branch (committing into memory cannot fail),
the vendor-body pass's disk round trip, and three on-disk record writers
(they now return text).

## The risks, one by one

| risk | what the run did before | what it does now | test |
| --- | --- | --- | --- |
| exit 1 on a rename-invariant failure | the tree was on disk when the closing reports set exit 1 | the tree is written right after the finish, before the closing reports decide the exit code; unchanged order of messages | `output_tree_test::a_built_tree_is_written_before_the_build_value_is_handed_on` |
| a stage fails after the tree has content (a crash, a split that fails before its commit) | what the stages had written so far stayed on disk | `build_then_write` writes the tree whatever the build returned; the stage's error is the run's error | `output_tree_test::a_failed_build_still_writes_what_it_built_and_returns_its_error` |
| files already in the output folder | never deleted (except a path the run itself wrote then removed); the finish read the folder as it stood: a leftover under `vendor/` named in a WARNING, a leftover `.js` outside `node_modules`/`.humanify`/root `embedded/` desugared for `using` in place and its requires listed in `package.json` | the same: reads and folder listings the tree does not answer go to the disk (`FolderReader`), so the finish sees disk + this run's files − its removals; the writer touches only the run's files and removals | `pipeline_stages::a_run_into_a_folder_with_leftovers_keeps_and_reads_them_as_on_disk` (end to end; passes on origin/main and on this branch), `pipeline_stages::a_rerun_into_the_same_folder_never_relinks_the_earlier_runs_vendor_files` (unchanged), core `output_tree_test` (the overlay) |
| prior folder == output folder | exp248 loads the whole prior before the first write | unchanged, and now nothing is written until the end at all; the finish's listing of the folder sees the prior's files as leftovers exactly as before | the leftovers test above covers the listing |
| memory | the tree lived on disk | the tree lives in memory until the write; the dispatch log still streams to its part files, untouched; the tree is dropped right after the write | peak RSS below |
| library detection | read unpacked files through the shell's reader (`index_files::read_utf8`) | reads them from the tree (`OutputTree::read`); the reader is deleted | the pipeline tests and the e2e |

Two behaviour notes, neither reachable on the pairs:

- A disk error while writing (a full disk, a folder that cannot be made)
  now surfaces at the one write, as a crash naming the path. Before, the
  same error during the finish ended the finish with "Post-split step
  failed (…)" and exit 0. A missing embedded file is the same case: its
  copy is made by the writer.
- The finish's walks list a folder sorted by name instead of in
  `read_dir` order. Every consumer is per-file or order-free (the
  externals are sorted); only which of two failing files is named first
  could differ.

The webcrack shim is a subprocess that writes its files straight into the
output folder; the tree reads them from there (as it reads leftovers),
and the run's own rewrites of them go through the tree.

## Gate

`cargo dev check` on the final tree:

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW (advisory; this lane's one finding, the core test disk twin of the shell's `Disk::list`, allowlisted with its reason) |
| census:concurrency | PASS |
| census:effects | PASS (264 entries) |
| census:slop | PASS |
| census:comments | PASS (no new findings) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (fresh and `--prior-version`, every launch equal to the golden) |

Red seen before green: the core and cli tests that drive the stages
failed to compile against the new signatures until each test built a
tree (and the core's test-only disk helper existed); the end-to-end
leftovers test was written first as a characterization test and run on
origin/main, where it passes (the behaviour it pins is today's).

## Neutrality

`cargo dev eval neutrality 1e1c69628199 <pair> --priors /work/neutrality-priors
--cache /work/neutrality-cache` on the committed candidate `a6d54103e`,
one at a time under `/work/neutrality.lock`:

| pair | differing files | differing lines | exit codes | cache writes (candidate / baseline) | verdict |
| --- | ---: | ---: | --- | --- | --- |
| 2.1.118 → 2.1.119 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |
| 2.1.197 → 2.1.198 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |

Logs: `/work/exp249-neutrality-118.log`, `/work/exp249-neutrality-197.log`.

## Peak memory

Each binary run by hand on 2.1.197 → 2.1.198 with the neutrality leg's
flags and config (warm cache, standing prior), one run at a time under
the neutrality lock, peak RSS from `getrusage(RUSAGE_CHILDREN)`
(`/work/exp249-rss/peak.py`, log `/work/exp249-rss/peak.log`). Base is
origin/main `1e1c69628` built by `cargo dev build-bin`.

| run | origin/main | this branch |
| --- | ---: | ---: |
| 1 | 16,031 MB | 16,059 MB |
| 2 | 16,007 MB | 16,141 MB |

The two output trees are byte-identical (`diff -r`, 113 MB each). The
difference (+28 to +134 MB) is the order of the tree's size, as
expected: it is held from the unpack to the write, and the run's peak is
elsewhere (the naming waves). The dispatch log is unchanged (it streams).

## Open items

- The artifact dump (`artifact_dump.rs`) still writes its own files
  from core: step G.
- The input reader and the chunk linker (`unpack/input.rs`,
  `unpack/chunk_link*`) still read the input bundle's files from core.
- The webcrack shim writes into the output folder itself; giving it a
  scratch folder and reading its files into the tree would make the tree
  the only writer, at the cost of a copy.
