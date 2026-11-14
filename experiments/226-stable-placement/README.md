# exp226 — an unchanged statement stays in the file it was in

STATUS: BUILT (TDD), GATED (all 11 stages pass), A/B MEASURED on three hops
with the model's answers pinned (main and candidate replay one cache; the
candidate wrote 0 entries on every hop). No eval, no walk. Branch
`exp226-stable-placement` from main `70884aef1`. No Claude Code text is
committed; shapes are described generically.

## Totals

2.1.238 -> 2.1.239, the clean A/B: both legs start from a copy of the
coordinator's control tree `ctl-2.1.238` (written by main), so neither leg
is a transition. Main ran first and filled a fresh LLM cache; the candidate
replayed it and wrote 0 entries, so both legs got the same answers and the
only difference is this change. Classifier: the noise study's
`tree_stmt.py`, grouped as in `rollup2.py`. Numbers are lines, then per 100
REAL lines.

| bucket | main `70884aef` | **exp226** | change |
| --- | ---: | ---: | ---: |
| REAL (classifier) | 61,210 | **61,198** | -12 |
| noise | 9,990 | **9,034** | -956 |
| **noise per 100 real** | 16.32 | **14.76** | -1.56 |
| MOVE across files | 1,181 (1.93) | **551 (0.90)** | -630 |
| RENAME module-alias, unchanged statement | 456 (0.74) | **324 (0.53)** | -132 |
| RENAME module-alias, inside edited statement | 396 (0.65) | **292 (0.48)** | -104 |
| WIRING require: added/removed for unchanged code | 498 (0.81) | **392 (0.64)** | -106 |
| WIRING export: line moved to other file | 134 (0.22) | **80 (0.13)** | -54 |
| WIRING export: added/removed for unchanged code | 111 (0.18) | 109 (0.18) | -2 |
| WIRING require: line moved to other file | 14 (0.02) | 16 (0.03) | +2 |
| MOVE in file: statement order changed | 773 (1.26) | 814 (1.33) | +41 |
| MOVE in file: require lines reordered | 182 (0.30) | 206 (0.34) | +24 |
| exit / `--version` / `-p boot-ok` | 0 | **0 / pass / pass** | |

- The main leg agrees with the coordinator's cold control on the same hop
  (`s-ctl-238-239.json`: 16.11 per 100, MOVE across 1,094); the pinned
  main leg reads 16.32.
- 42 statements changed file between the two legs. In every one, the
  name the candidate wrote is the name the prior had for that statement.
- Only 2 statements whose hash is unchanged still change file between
  2.1.238 and the candidate's 2.1.239; both move between `src/` and
  `vendor/` (left alone on purpose, below). The 551 MOVE lines left are
  EDITED statements whose module moved (one 381-line function is most of
  them). Their hash changed, so this rule has no evidence for them.
- The two small rises (+41 statement order, +24 require order) are
  ordering inside a file: a kept statement sits in its old file next to
  statements the bundle now emits in a different order. That is exp227's
  question.

Two more hops, each a transition (the prior is an OLD walk tree, so both
legs also re-indent and rename everything main changed since). Same
protocol: one prior, main fills the cache, the candidate replays it with
0 writes. Only the placement buckets are readable here; the totals are
dominated by the transition.

| bucket | 2.1.215->216 main | **exp226** | 2.1.300->301 main | **exp226** |
| --- | ---: | ---: | ---: | ---: |
| REAL | 53,661 | 53,957 | 124,474 | 124,100 |
| MOVE across files | 1,767 | **231** | 11,621 | **11,136** |
| RENAME module-alias, unchanged | 522 | **242** | 3,676 | **1,304** |
| RENAME module-alias, inside edited | 312 | **122** | 601 | **151** |
| WIRING require: for unchanged code | 489 | **211** | 1,916 | **1,792** |
| WIRING export: moved to other file | 200 | **38** | 296 | **228** |
| WIRING export: for unchanged code | 43 | 75 | 66 | 69 |
| REINDENT | 1,148,426 | 1,150,020 | 1,922,242 | 1,923,857 |
| statements refiled by this change | | 98 | | 56 |

- 2.1.215 -> 2.1.216: MOVE across files falls by 87%. REINDENT rises by
  about the same number of lines (+1,594). This is the transition: a
  statement that moved file was charged as a MOVE and its re-indent came
  with it. Kept in its file, the same statement now shows as re-indented
  (the old tree nested it, main's writer does not). In a steady-state hop
  there is no re-indent to show. REAL +296 is the classifier relabelling
  lines inside edited statements in the files that now keep more code
  (edited:real +1,000, rename:same-line -1,016). The clean 238 -> 239 A/B
  shows REAL -12.
- 2.1.300 -> 2.1.301 (split era): the module-alias lines fall by 2,822,
  but MOVE across files falls by only 485. The moves left are whole
  chunk modules that changed file. Their code sits in the module's init,
  which this change never moves. The old walk's prior names its chunk
  files by position counter (before exp221), so the module matcher pairs
  differently on this hop. Measuring the split era properly needs a prior
  written by main; that needs two more model-backed hops, which went past
  this lane's budget.

## The problem

The marker split (`place::assign::fossil`) cuts the bundle into modules at
each `__esm` init: every statement after the previous init belongs to the
next init's module, and a module is written as one file. A run of
declarations with no init of its own (an init-less source module, or a
module's hoisted functions) is therefore glued to whichever module the
bundler emits NEXT.

Between two releases the bundler can emit modules in a different order
(a new import moves a module up), or a new init can appear between the
run and the module it used to join. The run then joins another module,
and the whole run changes file, although none of it changed. A traced
example: on 2.1.215 -> 2.1.216, a run of five unchanged helper functions
sat in front of module A in both bundles. In 2.1.216 the bundler emitted
another module, B, between the run and A, so the helpers were written to
B's file. Every importer
then changed its module alias (`aliasA.fn` to `aliasB.fn`), and require
and export lines were added and removed.

exp215 handled one case: an UNMATCHED module whose carried statements all
lived in one prior file keeps that file. It did not cover a run glued to
a module that IS matched (to its own, different, prior file), which is
the common case above.

## The fix

After every module has its file (library paths, the prior match, exp215's
keep, fresh names), `keep_carried_statements` looks at each statement:

- the statement's hash appears exactly once in this bundle, and the prior
  held it in exactly one file (`prior_hash_files`);
- that prior file is not the module's file;

then the statement goes back to its prior file. The module's own init
always stays with the module. Library modules (names mode), `vendor/`
paths and the entry file `src/index.js` are never refiled, in either
direction. The trail records such a statement as `fossil:kept-statement`,
and the split's summary counts them (`N statements kept in their prior
file`).

Two details matter:

- **The prior file is read from the prior's per-statement layout.** The
  ledger's `hashes` + `order` lists say which file each statement was
  actually written to. Reading module files instead would undo the fix on
  the next hop: a refiled statement's module names a different file. A
  ledger without that layout (or one whose layout never wrote a file its
  modules name: only a hand-edited ledger does that, such as one frozen
  TS-era replay case) falls back to the modules' files.
- **The hash must be unique in this bundle.** The statement hash ignores
  names, so small shapes collide (a two-line predicate, a `var a, b, c;`
  list with the same count). The first build only required one prior
  file. On 238 -> 239 it refiled 83 statements, and 37 of them were NEW
  statements that shared a carried statement's shape. REAL rose by 850
  lines (code split away from its module). With the uniqueness check,
  REAL is -12, and the refiled statements carry the prior's own names: 42
  of 42 on 238 -> 239, 56 of 56 on 300 -> 301, 95 of 98 on 215 -> 216
  (in two of the other three only a parameter was renamed, and the third
  is a long `var` list).

The emitter already handles statements of one module in several files:
references across files become `require` + member access, and writes
across files get a setter. No emit change was needed, and no run declined
the runnable emit.

Files:

- `crates/humanify-ag-core/src/place/assign/fossil.rs`: `keep_carried_statements`,
  `prior_hash_files` (now reads the per-statement layout), the trail
  tier, `FossilStats.kept_statements`.
- `crates/humanify-ag-core/src/place/placement_dump.rs`: the summary line.
- `crates/humanify-ag-core/src/place/assign/fossil/fossil_test.rs`: five
  new tests, three exp215-era tests updated to the new behaviour.
- `crates/humanify-ag-core/src/emit/stable_split/stable_split_test.rs`:
  one end-to-end test through the runnable emit.
- `docs/responsibility.md`: the existing "which prior file did this
  statement live in" row now covers the new step.

## Tests (red, then green)

| test | before | after |
| --- | --- | --- |
| `an_unchanged_run_glued_to_another_module_stays_in_its_prior_file` | FAIL (helpers in the limits file) | pass |
| `a_kept_statement_holds_on_the_next_hop` | FAIL | pass |
| `the_trail_records_a_kept_statement_with_its_own_tier` | FAIL | pass |
| `a_statement_never_returns_to_a_vendor_file` | pass (guard) | pass |
| `a_statement_whose_shape_this_bundle_holds_twice_is_not_refiled` | FAIL (written against the first build) | pass |
| `an_unchanged_run_glued_to_another_module_keeps_its_file_in_the_runnable_tree` | FAIL (`src/limits.js`) | pass |

Updated, because the behaviour they pinned is the one this change
replaces (each now asserts the module is still named fresh, and its
carried statements stay where they were):
`a_statement_whose_hash_lived_in_another_prior_module_stays_there`
(was `..._reads_as_a_move`), `a_module_whose_carried_statements_came_from_two_prior_files_is_named_fresh`,
`a_mostly_new_module_is_named_fresh`. The frozen TS split-call replay
(`every_captured_ts_split_call_replays_exactly`) passes unchanged.

## Gate

`cargo dev check` in the worktree, on the final code.

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW (two twins in `humanify-ag-dev`, both already on main; none in this change) |
| census:concurrency | PASS |
| census:slop | PASS |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 fixture pairs, 176 launches equal to the golden) |

## How it was measured

- Binary: main `/work/main-70884aef-frozen/target/release/humanify-ag`;
  candidate built from this branch (`/work/exp226-runs/bin/humanify-ag-v2`).
- Launch: the control's (`/work/control-70884aef/run.sh`) plus
  `llm.cache_dir`, one cache per hop, main first. Cache files before ->
  after the candidate leg: 6,271 -> 6,271 (239), 1,939 -> 1,939 (216),
  2,907 -> 2,907 (301). This pins the model's answers. That is allowed
  here (measurement-pitfalls rules 10/11) because the change runs after
  every prompt, and the zero writes show it asked nothing new.
- Trees, classifier output and scripts: `/work/exp226-runs/ab/`
  (`s-main-*.json`, `s-mine2*.json`), `/work/exp226-runs/ana/`.

## Not fixed, noted

- **Edited statements whose module moved.** Their hash changed, so there
  is no hash evidence. On 238 -> 239 they are most of the 551 MOVE lines
  left. The naming stage's function match (prior-match map) pairs an
  edited function with its prior one, and could carry the file the same
  way. Not attempted.
- **Statements whose shape repeats in the bundle** are not refiled (no
  evidence which copy is which). A neighbour rule (a repeated statement
  between two refiled statements of the same file joins them) could take
  some of these. Not measured.
- **Whole modules filed elsewhere** (the split era's remaining moves): an
  init's own hash could vote for the module's file when the matcher
  disagrees. That is the module matcher's question, not this step's.
- **`vendor/` <-> `src/` moves** are left to the library placement on
  purpose (2 statements on 238 -> 239).
- **Overlap with exp227** (order within a file): a kept statement can sit
  out of order in its old file (+41 / +24 lines on 238 -> 239).
- The file count rose by 1 on 238 -> 239 (5,308 -> 5,309): a prior file
  whose module went away keeps the carried statements it held.

## What this cannot prove

- One clean A/B hop (238 -> 239). The other two hops are transitions from
  old walk trees, and only their placement buckets are readable.
- No steady-state split-era number: the 300 -> 301 prior was written by
  an older binary.
- The rule can keep code in an old file when upstream really moved it to
  a new source file. A minified bundle does not show upstream file names,
  so the split cannot tell the two apart. Keeping the file is the smaller
  diff (the same trade exp215 made).
