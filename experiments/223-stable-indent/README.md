# exp223: every split statement is indented at its own depth

STATUS: BUILT (TDD), GATED (all 11 stages pass), FOUR REAL HOPS (live
model, all exit 0, all boot both halves). No eval, no walk. Branch
`exp223-stable-indent` from main `d3fed6a41`. No Claude Code text is
committed. The shapes below are described generically.

## Totals

Classifier (`/work/noise-study/tree_stmt.py`, roll-up as in `rollup2.py`)
on 2.1.238 -> 2.1.239. Both runs are steady-state. The walk's trees come
from main. exp223's 2.1.239 was built from exp223's own 2.1.238, which
was itself built by exp223 from the walk's 2.1.237.

| 2.1.238 -> 2.1.239 | walk (main `73d01430`) | exp223 | change |
| --- | ---: | ---: | ---: |
| changed lines | 77,922 | 73,496 | -4,426 |
| REAL lines | 62,641 | 62,028 | -613 (explained below) |
| noise lines | 15,281 | 11,468 | **-3,813** |
| **noise per 100 real** | **24.4** | **18.5** | **-5.9** |
| reindent bucket | 6,799 (10.85 per 100) | 3,356 (5.41 per 100) | -3,443 |
| reindent lines in UNCHANGED statements | 1,813 | 23 (not indentation, see below) | -1,790 |

Files with a top-level function whose body sits one level too deep (the
symptom: `function f() {` at column 0, body at 4, `}` at 2):

| tree | walk (main) | exp223 |
| --- | ---: | ---: |
| 2.1.239 `src/` (5,308 files) | 3,927 | 0 (*) |
| 2.1.239 `vendor/` (2,325 files) | 258 | 0 |
| 2.1.301 `src/` (2,098 files, split era) | 1,412 | 0 (*) |
| 2.1.301 `vendor/` (1,495 files) | 9 | 0 |

(*) One file in each still matches the probe. In both cases the match is
source text held inside a template literal, which is kept byte for byte
on purpose.

| real hop (binary from this branch) | exit | wall | `--version` | `-p boot-ok` |
| --- | --- | ---: | --- | --- |
| 2.1.238 -> 2.1.239 from the walk's 2.1.238 (transition) | 0 | 1,143 s | pass | pass |
| 2.1.237 -> 2.1.238 from the walk's 2.1.237 (transition) | 0 | 1,029 s | (not run) | (not run) |
| 2.1.238 -> 2.1.239 from exp223's own 2.1.238 (steady state) | 0 | 1,032 s | pass | pass |
| 2.1.300 -> 2.1.301 from the walk's 2.1.300 (transition, split era) | 0 | 958 s | pass | pass |

The wall times are longer than the walk's (764 s for 239) because other
lanes shared the LLM server at the same time.

## The cause

The runnable split emit (`emit::cjs`, `Plan::stmt_text`) wrote each
top-level statement as a byte slice of the formatted bundle. In the bundle,
every app statement sits inside the bundle's wrapper function, one level
(2 spaces) deep. A slice starts at the statement's first character, so its
first line landed at column 0 while every following line kept the wrapper's
2 extra spaces. That gave the body at 4 and the closing `}` at 2.

This was consistent from version to version, so most hops did not show it.
It became noise when a file was **reprinted**. The `using` desugaring
(`finish::using`) regenerates only the files that declare with `using`, and
it prints them at their true depth. When a file starts or stops declaring
with `using`, every multi-line statement in it re-indents. On 2.1.238 ->
2.1.239, 7 files crossed that line, and they carried 4,707 of the 6,799
reindent lines (1,790 of them in statements that did not change at all).
More files use `using` later in the walk, which explains why the churn
grows over the walk.

## The fix

`emit::cjs` now writes every statement through the existing owner for
"the source slice re-indented to column 0":
`naming::waves::generate::TextView::pretty`, the view the prompts already
use. A new helper, `reindented(text, start, end, edits)`, replaces
`apply_edits`:

- every line moves left by the indentation of the line the node starts on;
- a line that begins inside a template quasi or a string literal is
  content and is copied byte for byte (the view's raw regions);
- the identifier rewrites (cross-file reads, `delete`, shorthand) are
  applied in the same pass, and overlapping rewrites still fail as before;
- the cross-file redeclaration composites (`ns.x = <init>`, a declarator
  kept as its own declaration) go through the same path.

The output is the same program with different leading spaces. e2e imports
every output and checks its exports, and that stage passes.

Not changed: the review tree (`emit::review`). It is written only with
`--split-pure` or when the runnable emit declines, and its byte-exact
slices back the concat-equivalence check. Files reprinted by the `using`
desugaring keep that printer's own style.

## What is left in the reindent bucket (3,356 lines)

| where | walk | exp223 |
| --- | ---: | ---: |
| unchanged statements, `using`-reprint files | 1,790 | **0** |
| edited statements, `using`-reprint files | 2,917 | 1,264 |
| edited statements, other files | 2,069 | 2,069 |
| unchanged statements, other files | 23 | 23 |

- **Edited statements, other files (2,069, identical in both):** the source
  itself changed how deeply this code is nested (for example, code wrapped
  in a new block). The classifier counts a line whose stripped text matches
  but whose indentation changed as reindent. This is a source-level change
  and out of this lane's reach.
- **Edited statements in `using`-reprint files (1,264):** the desugaring
  wraps the `using` scope in `try`/`finally`, so the code inside goes one
  level deeper. That is a real consequence of the desugaring, not
  slice indentation.
- **23 lines in unchanged statements:** classifier pairing artifacts in
  class bodies (re-rolled `#x` private members and build constants that
  match another line's stripped text). They are not indentation.

## REAL lines (-613)

- -320 are in the 7 `using`-reprint files. Before the fix, a line there
  differed in both indentation and printer style, and the classifier sent
  some of those lines to REAL.
- The other -290 net is spread over 206 files. It comes in equal and
  opposite pairs (for example +1,037 / -1,037, +701 / -701): new files
  that the model named differently on this cold run. The content is the
  same and only the file name moved. That is LLM draw, not this change.

## The one-time transition

The first hop on a prior written by an older binary compares mis-indented
files with re-indented ones. On 2.1.238 (walk) -> 2.1.239 (exp223), the
classifier puts **1,537,282 lines in the reindent bucket, once**.

The post-split reconcile (`finish::reconcile`) diffs each file against the
prior's file. On that transition hop, almost every multi-line statement's
hunk grows past the reconcile's 10-line limit, so the reconcile restores far
fewer prior names. The walk restores about 5,400 to 6,300 per hop, most of
them top-level names it can never carry into the bundle and re-restores on
every hop. exp223 restored 304 on its transition. The names that were not
restored keep the bundle's names from then on: a one-time rename wave. On
the steady-state hop the reconcile restored 871, and the classifier's
rename buckets did not rise (renames of locals in edited statements
1,912 -> 1,649; in unchanged statements 162 -> 46).

## Gate

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones (advisory) | REVIEW: 2 groups, both in `humanify-ag-dev`, older than this branch |
| census:concurrency | PASS |
| census:slop (advisory) | PASS |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 pairs, launch golden equal) |

Tests:

- New: `emit::cjs::cjs_test::statements_are_written_at_their_own_depth_with_literal_lines_kept`.
  It failed before the fix: the body was at 4 and `}` at 2. It checks a
  template literal's lines and a string's continuation line stay byte for
  byte, and that a rewritten cross-file statement is re-indented too.
- Re-recorded: 9 emitted files across 9 vectors of `test/parity/wp53-cjs.json`
  (the frozen TS emit vectors). A script checked each change before the
  re-record: same line count, and every changed line differs only by
  leading spaces removed.
- The e2e goldens did not change. The legacy goldens hold unsplit
  single-file outputs, and the launch golden holds launch lines, not tree
  bytes. The split trees are held to exit, determinism and export surface,
  and those pass.

## Open items

- The scorer (`novel` / `realLn`) was not run. That needs an eval, which
  needs the coordinator's OK. Expect the first eval label after this
  change to need a re-scored or re-run reference, because every scratch
  base and scored tree changes indentation.
- No steady-state split-era read. 2.1.300 -> 2.1.301 was a transition,
  so its classifier numbers would mostly measure the transition. The
  structural probe above shows the split era's 1,412 affected files go
  to 0.
- The remaining `using`-reprint style churn: a file that starts declaring
  with `using` still switches to the desugaring printer's style (arrow
  parens, line wrapping), about 2,560 changed lines across 7 files on
  238 -> 239. One fix would be to splice the desugaring into the emitted
  text instead of regenerating the whole file. That is a separate change.
- The review-tree fallback still writes byte-exact slices.

## Reproduce

Runs live in `/work/exp223-runs/` (binary `bin/humanify-ag`, launch script
`scripts/hop.sh <bin> <srcWalk> <version> <prior>`, trees under `walk/trees`,
classifier outputs under `cls/`). Per-file reindent attribution used a copy
of the classifier with per-file counters (`scripts/tree_stmt_perfile.py`).
