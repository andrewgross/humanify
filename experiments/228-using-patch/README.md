# exp228: the `using` desugar edits the file in place instead of reprinting it

STATUS: BUILT (TDD), GATED (all 11 stages pass), TWO REAL HOPS (live
model, both exit 0, the 2.1.239 output boots both halves). No eval, no
walk. Branch `exp228-using-patch` from main `70884aef1`. No Claude Code
text is committed. The shapes below are described generically.

## Totals

Classifier (`/work/noise-study/tree_stmt.py`, roll-up as in `rollup2.py`)
on 2.1.238 -> 2.1.239. Both columns are two-hop chains from the old
walk's 2.1.237, so both hops are steady state for their binary: the
coordinator's control (main `70884aef1`: `ctl-2.1.238` -> `ctl-2.1.239`)
and the same chain run with this branch's binary.

| 2.1.238 -> 2.1.239 | main (control) | exp228 | change |
| --- | ---: | ---: | ---: |
| changed lines | 71,136 | 69,489 | -1,647 |
| REAL lines | 61,267 | 60,680 | -587 (explained below) |
| noise lines | 9,869 | 8,809 | **-1,060** |
| **noise per 100 real** | **16.1** | **14.5** | **-1.6** |
| reindent bucket | 3,287 (5.37 per 100) | 2,079 (3.43 per 100) | -1,208 |

The files that declare with `using` on either side (20 files, the same
20 in both runs: 13 in 2.1.238, 20 in 2.1.239, 7 of them new):

| lines in `using` files | main (control) | exp228 | change |
| --- | ---: | ---: | ---: |
| changed lines | 2,846 | 1,215 | -1,631 |
| REAL | 1,473 | 1,054 | -419 |
| noise | 1,373 | 161 | **-1,212** |
| reindent | 1,271 | 58 | -1,213 |
| noise per 100 real (of the whole hop) | 2.24 | 0.27 | -1.97 |

The 7 files that START declaring with `using` at 2.1.239 carry all of
it: 1,352 noise lines on main, 132 with exp228. The 13 files that
declare with `using` on both sides are unchanged (both runs print them
the same way on both sides).

Files that declare with `using` (and so are desugared):

| tree | files with `using` | all `.js` files |
| --- | ---: | ---: |
| 2.1.239 (control and exp228) | 20 | 7,636 |
| 2.1.301 (main's split-era walk, `walk-d3fed6a4-up`) | 38 | 3,596 |

| real hop (binary from this branch) | exit | wall | `--version` | `-p boot-ok` |
| --- | --- | ---: | --- | --- |
| 2.1.237 -> 2.1.238 from the old walk's 2.1.237 | 0 | ~12 min | (not run) | (not run) |
| 2.1.238 -> 2.1.239 from exp228's own 2.1.238 | 0 | ~13 min | pass | pass |

Every desugared file in both trees passes `node --check`.

## The cause

Bun cannot `require()` a CommonJS file that declares with `using`, and
Node before 24 cannot parse it, so the finishing step
(`finish::using`) compiles those declarations away, as Babel's
explicit-resource-management plugin does. It did that by REPRINTING the
whole file with the Babel-style printer under `retainLines`. That
printer has its own style (parentheses around a lone arrow parameter,
its own line wrapping, statements at their own depth). When a file
starts or stops declaring with `using` between two versions, every line
of it switches between the writer's style and the printer's style.

On top of that, the plugin wraps each block that holds a `using` in a
`try` / `finally`. The printer then indents the whole block body one
level deeper. exp223 measured that part at 1,264 reindent lines on this
hop.

## The fix

The desugar still builds the plugin's program, but it writes it as
SPLICES into the file's own text (`finish::using::transform`):

- `using x = init` becomes `const x = _usingCtx.u(init)` (`.a(...)` for
  `await using`): only the keyword is replaced and the call is put around
  the initializer (around its outer parentheses, if it has any).
- An anonymous function initializer still gets the name helper.
- The `try {var _usingCtx = _usingCtx2();` opener goes on the line of the
  block's `{`. The `} catch (_) {...} finally {...}` closer goes on the
  line of the block's `}`. These are the same two lines the old output
  changed. The body between them keeps its bytes and its indentation.
  For a function, `try`, `catch` or static-block body, the try goes
  inside the braces. For any other block, the block itself becomes the
  try block (`if (c) try {...} catch ... finally ... else ...`). That is
  the plugin's tree.
- The helpers are printed as before (compact, on one line) and go on a
  new first line above the first statement (after any directive
  prologue).
- The order of the `_usingCtx<n>` names and of the helpers is the
  plugin's order, as before.

Behaviour kept: the same files are desugared (token prefilter, then a
parse check). The same inputs are refused: `for (using ... of ...)`, a
program-scope binding that shadows a global the helpers read, and an
input that already spells a generated name. One refusal is gone:
comments. It existed only because the reprint could not place comments,
and a splice never moves them.

The output is not byte-equal to Babel's any more. Its program is. The
parity test (`desugar_builds_the_babel_program`, over the frozen
`test/parity/wp54-desugar.json`) prints both texts with the beautify
printer, which drops layout, and requires them equal for every case:
try/catch bodies, nested blocks, `async` ordering, eleven contexts,
static blocks, the name helper, the directive prologue, multi-line
arguments and templates. The comment case is now desugared, and the
test checks that its comment is kept.

## REAL lines (-587)

- -417 are in the 7 files that start declaring with `using`. Under the
  reprint, a line that changed for real AND switched printer style was
  sometimes counted as REAL where the writer's own text now pairs it
  with its old line. For example, an arrow that gained parentheses no
  longer matches as "same text". All 7 move the same way (for example
  328 -> 172 and 100 -> 41 REAL).
- The other -170 net is outside the `using` files. Both hops of both
  chains are cold runs with live model draws (the 2.1.238 hop too), so
  file and local names differ a little between the two chains. Noise
  outside the `using` files moved +152 the same way (renames in edited
  statements +104, cross-file moves +93). That is draw, not this
  change. This change cannot reach a file without `using`, because
  those bytes are never touched.

## What is left in the `using` files (161 noise lines)

- 58 reindent: lines of new source code whose stripped text matches a
  line elsewhere, for example an object key repeated in a new call. The
  desugar no longer re-indents anything, so these are classifier
  pairings, not indentation.
- About 100 renames and moves, the same kinds as in every other file
  (locals in edited statements, module aliases, export names).

## The one-time transition

The first hop of this binary on a prior written by an older binary
compares old reprinted files with spliced ones. On that hop, every file
that declares with `using` on both sides switches style once (13 files
at 2.1.238 -> 2.1.239, 38 files in the split era). That was not
measured: the A/B above was run as two hops on purpose, to avoid it.

## Gate

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones (advisory) | REVIEW: 50 groups, the same count as main's merge gates, none in `finish/using` |
| census:concurrency | PASS |
| census:slop (advisory) | PASS (every crate +0.0pp) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS |

Tests (`finish::using::using_test`):

- New, red first: `only_the_lines_the_desugar_must_change_are_touched`.
  On main it failed: the reprint added `(x) =>`, re-indented the body
  and moved the closer onto the last statement's line. It requires
  every untouched line to stay byte for byte, and checks a
  parenthesized sequence initializer, the name helper and the
  block-becomes-try form.
- Replaced: `desugar_matches_the_ts_bytes` became
  `desugar_builds_the_babel_program` (programs compared, not bytes; see
  above). It failed first on the comment case, which the old code
  refused.

## Open items

- The scorer (`novel` / `realLn`) was not run, because that needs an
  eval and the coordinator's OK. The first eval after this change
  re-desugars every `using` file of every base. Expect `using` files to
  read differently from the references once.
- No split-era read (2.1.300 -> 2.1.301). There are more `using` files
  there (38), so the effect per hop should be larger whenever files
  start or stop declaring with `using`.
- Overlap with other lanes: none in code (only `finish/using` changed).
  exp227 (order within files) does not reach these files' text: the
  desugar now keeps whatever order and bytes the writer produced.

## Reproduce

Runs live in `/work/exp228-runs/`:

- the binary: `bin/humanify-ag`
- the launch script: `hop.sh <version> <prior humanified.js> <name>`,
  the same launch as the control's `run.sh`
- the trees: `my-2.1.238` and `my-2.1.239`
- the classifier outputs: `cls/ctl-238-239.json` (equal to the
  coordinator's `s-ctl-238-239.json`) and `cls/my-238-239.json`

The classifier was a copy with per-file counters
(`scripts/tree_stmt_perfile.py`), and the roll-up was
`scripts/roll.py <cls.json> <treeA> <treeB>`.
