# exp251 — a run that cannot write the runnable tree fails, and placement no longer makes the load-time cycle that caused it

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages). Neutrality: see
"Neutrality" below. Branch `exp251-runnable-emit-fails`, not merged.

## Totals

- **A declined runnable emit now fails the run.** The pipeline prints one
  line, `ERROR: no runnable tree was written (<reason>); only the
  byte-exact review tree was written, and this run is marked failed.`,
  still writes the review tree, and exits 1. Before, it printed
  `Runnable emit declined: ...` and exited 0. The reason names the files,
  e.g. `runnable emit: load-time reference cycle: src/a.js -> src/b.js ->
  src/a.js`.
- **The harness treats that exit as a failure.** `eval score` lists a
  scored leg that exits non-zero as a failed pair in its closing `!! eval
  score FAILED` verdict (the card is still scored for inspection) and
  refuses a rebase that exits non-zero even when it wrote a tree. `eval
  walk`'s report flags every hop that exited non-zero. `eval neutrality`
  already compared exit codes.
- **A load-time cycle guard in the split.** When the runnable emit
  declines on a load-time cycle between files, the split moves the fewest
  app statements that break it and writes the split again. On the walk's
  failing release it moves one function back to the file it was in the
  release before, and the runnable emit succeeds.
- No change where no cycle forms: the guard only runs after the emit has
  declined on a cycle.

## What happened (the reproduction)

A walk of an older binary (3cb20421) wrote only the review tree at Claude
Code 2.1.302 and, inheriting its layout, at 2.1.303, and exited 0 both
times. Re-running that binary on the same prior (the walk's 2.1.301 tree)
reproduced it exactly:

    Runnable emit declined: runnable emit: load-time reference cycle:
    src/windows-path-utils.js -> src/path-to-file-url.js -> src/windows-path-utils.js
    EXIT 0

The placement trail of that run says why:

| statement | placed by | file |
| --- | --- | --- |
| the lazy-init helper (`__esm`) | kept: its hash was held by one prior file | windows-path-utils.js |
| `function getCurrentWorkingDirectoryPath()` (body changed in 2.1.302) | its graph neighbours (no hash match) | path-to-file-url.js |
| `var cwdPath = getCurrentWorkingDirectoryPath();` (unchanged) | kept: hash held by one prior file | windows-path-utils.js |
| an init built with the helper (`initAppWatcher = __esm(...)`) | kept | path-to-file-url.js |

In 2.1.301 the function and its caller were both in windows-path-utils.js.
In 2.1.302 the function's body changed, so its hash no longer matched and
the graph neighbours put it in path-to-file-url.js, while its unchanged
load-time caller was kept in windows-path-utils.js. Now
windows-path-utils.js calls path-to-file-url.js while it loads, and
path-to-file-url.js calls the helper in windows-path-utils.js while it
loads: a cycle the runnable emit cannot write.

Current main did not form this cycle on 2.1.301->2.1.302 (its own cold run
booted), but nothing in placement prevents the same split of a function
from its load-time caller, so the guard is wanted regardless.

## The guard

`core::emit::cycle_guard::break_load_time_cycles`, on
`core::emit::cjs::load_graph`:

1. `load_graph` lists every load-time read between top-level statements,
   whatever file each is in (the same reads the emit's cycle check uses:
   reference and write sites outside function bodies, `delete x` excluded)
   and the namespace augmentations the emit moves next to their target.
2. The guard finds the files on a cycle (strongly connected components)
   and, for every file-to-file edge inside one, tries two moves: the
   edge's readers join the file they read, or the declarations they read
   join the readers' file. It takes the move that leaves fewer files on a
   cycle, then empties no file, then leaves the fewest load-time reads
   across files, then moves fewer statements, then moves declarations
   rather than readers. It repeats until no cycle is left (at most 16
   rounds) or no move helps.
3. A statement under `vendor/`, or one that would move into it, stays put;
   no statement moves twice. If the guard cannot break the cycle, the
   emit's decline stands and the run fails with exit 1.
4. The split is written again from the new placement (review tree,
   ledger, vendor bridges, runnable emit). The moves are counted in the
   placement trail under `load-time-cycle-guard` and printed as one
   `Load-time cycle guard: moved N statement(s) ...` line.

On the walk's 2.1.302 and 2.1.303 trees (their shipped text and ledger
order, offline):

    BEFORE: Err("runnable emit: load-time reference cycle: src/windows-path-utils.js -> src/path-to-file-url.js -> src/windows-path-utils.js")
    MOVED 6 src/path-to-file-url.js -> src/windows-path-utils.js: function getCurrentWorkingDirectoryPath() { ...
    AFTER: Ok(4873 files)

One move, the function back next to its caller, where 2.1.301 had it.

Tests: `cycle_guard_test` (no cycle, two- and three-file cycles, a move
that would only turn the cycle around, vendor statements, a namespace
augmentation moving with its target, and the 2.1.302 shape as a minimal
graph), `stable_split_test` (the old "load-time cycle declines" layout now
writes a runnable tree with one move; a function-redeclaration decline
still writes the review tree), `report_test` and `unified_test` (exit 1
and one ERROR line), `score_test` (a non-zero scored leg or rebase fails
its pair), `walk/report_test` (a non-zero hop is flagged).

## Neutrality

PENDING
