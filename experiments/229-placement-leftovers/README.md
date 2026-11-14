# exp229 — placement leftovers: modules handed to the wrong prior file, and statements the hash cannot place

STATUS: BUILT (TDD), GATED (see Gate), A/B MEASURED on one hop
(2.1.238 -> 2.1.239) with the model's answers pinned (the candidate replayed
main's cache and wrote 0 entries). Boots (both halves). No eval, no walk.
Branch `exp229-placement-leftovers` from main `3cb20421d`. No Claude Code
text is committed; shapes are described generically.

## Totals

2.1.238 -> 2.1.239, same-answers A/B. Both legs start from a copy of the
walk tree `walk-3cb20421-up/trees/2.1.238` (written by main). Main
(`final-3cb20421-frozen`) ran first and filled a fresh LLM cache; the
candidate replayed it: cache files 5,806 -> 5,806. Classifier: the noise
study's `tree_stmt.py`, grouped as in `rollup2.py`. Lines, then per 100
REAL lines.

| bucket | main `3cb20421` | **exp229** | change |
| --- | ---: | ---: | ---: |
| REAL (classifier) | 60,489 | **60,447** | -42 |
| noise | 6,892 | **6,068** | -824 |
| **noise per 100 real** | 11.39 | **10.04** | -1.35 |
| placement family (rows below) | 2,026 (3.35) | **1,124 (1.86)** | -902 |
| MOVE across files | 593 (0.98) | **233 (0.39)** | -360 |
| RENAME module-alias, unchanged statement | 326 (0.54) | **100 (0.17)** | -226 |
| RENAME module-alias, inside edited statement | 292 (0.48) | **130 (0.22)** | -162 |
| WIRING require: added/removed for unchanged code | 414 (0.68) | **293 (0.48)** | -121 |
| WIRING require: line renamed | 150 (0.25) | 136 (0.22) | -14 |
| WIRING require: line moved to other file | 12 (0.02) | 4 (0.01) | -8 |
| WIRING export: line moved to other file | 84 (0.14) | **20 (0.03)** | -64 |
| WIRING export: added/removed for unchanged code | 128 (0.21) | 179 (0.30) | **+51** |
| WIRING export: line renamed | 24 (0.04) | 28 (0.05) | +4 |
| RENAME module-alias, unchanged, moved file | 3 | 1 | -2 |
| RENAME local, unchanged statement (not placement) | 192 (0.32) | 214 (0.35) | +22 |
| whole tree, `git diff --shortstat` lines | 194,255 | 193,359 | -896 |
| exit / `--version` / `-p boot-ok` | 0 | **0 / pass / pass** | |

- The main leg agrees with the walk's own hop (`s-w5-238-239.json`:
  11.29 per 100, MOVE across files 575).
- REAL -42: edited statements that now stay in their file are diffed in
  place instead of as a removal plus an addition.
- The +51 export lines: a statement kept in its old file while its module
  went to another now needs an export for the code that uses it. It is
  the price of the -360 MOVE lines and -388 alias lines.

## What was wrong (measured before writing code)

Every statement that changed file on the walk's 238 -> 239 hop was
bucketed by why, from the two ledgers (`hashes`, `order`, `emitNames`,
`fossilModules`) and the classifier's per-statement dump (1,162 lines on
those statements, both sides):

| why | lines | statements |
| --- | ---: | ---: |
| EDITED statement (hash changed) whose module moved | 644 | 14 |
| wiring statements (requires, export blocks) following a move | 267 | 253 |
| hash NOT unique (exp226 skips it), incl. inits and `var x;` | 175 | 76 |
| `vendor/` <-> `src/` (library placement, left alone) | 69 | 8 |
| hash unique, still moved | 7 | 4 |

The module-alias renames were traced to their targets: most point at a
statement that moved for one of those reasons (one non-unique `var`, one
non-unique init-shaped function and its module were 150 of the 324 lines).

Replaying the module matcher offline on the two ledgers (the fresh
signatures are in the new ledger) reproduced main's decisions exactly and
showed four wrong pairs: two pairs of modules SWAPPED files, one unchanged
module was handed to another prior module, and a one-statement init
module took its neighbour's file. In each, an earlier tier paired a fresh
module with a prior module of similar SHAPE whose declared names it
shares none of, while another prior module carries its names (and code).

## The fix

1. **Matcher veto, both directions** (`fossil_match.rs`,
   `export_heir_veto`). A pair whose declared names are disjoint was
   refused only when the PRIOR module had a better fresh heir by names. It
   is now also refused when the FRESH module has a better prior one
   (names Jaccard >= 0.6). On the replay this fixes the four wrong pairs.
   The frozen TS matcher vectors (`wp51-fossil-match.json`) all still pass.
2. **Names as evidence for a statement's prior file** (`fossil.rs`,
   `CarriedEvidence`). exp226 refiles a statement only by its hash. When
   the hash gives no evidence (the bundle holds the shape twice, or the
   statement was edited), the statement's declared names do: every name it
   declares is declared once in this bundle and by one prior statement,
   all in one prior file (`nameToFiles`). One guard: a name is NOT evidence
   when the prior statement that declared it is still in this bundle as a
   different statement. The first build lacked it, and REAL rose by 2,100:
   the names a statement has at placement time are not final yet (the
   per-file reconcile renames afterwards), and a large new class wore a
   name the prior gave to a `var` list that still exists. With the guard,
   REAL is -42. The same evidence feeds exp215's whole-module keep
   (`kept_prior_file`, 12 statements kept that way on this hop).
3. **An export registration follows its namespace.** Its shape repeats
   for every module with the same number of exports, so its hash never
   says where it lived; it goes where the statement declaring its
   namespace went.

Inits, library modules, `vendor/` paths and the entry file are still never
refiled. Trail tiers: `fossil:kept-statement` 315 -> 383,
`fossil:kept-by-content` 0 -> 12.

## Tests (red, then green)

| test | before | after |
| --- | --- | --- |
| `a_fresh_module_is_not_paired_against_a_prior_module_its_names_disown` (matcher) | FAIL (paired by overlap) | pass |
| `an_edited_statement_split_into_a_new_module_stays_in_its_prior_file` | FAIL | pass |
| `a_statement_whose_shape_this_bundle_holds_twice_is_refiled_by_its_name` (was `..._is_not_refiled`, exp226) | FAIL | pass |
| `an_export_registration_follows_its_namespace` | FAIL | pass |
| `a_new_statement_wearing_a_prior_name_whose_code_is_still_here_stays_put` | FAIL (first build) | pass |
| `a_name_two_statements_of_this_bundle_declare_files_nothing` | guard | pass |
| `a_statement_refiled_by_name_holds_on_the_next_hop` | guard | pass |

The test ledger helper now writes `nameToFiles`, `emitNames` and
`emitHashes` the way the split does.

## Gate

`cargo dev check` in the worktree, on the final code: ALL 11 STAGES PASSED.

| stage | result |
| --- | --- |
| shims, no-private-addresses, rust:fmt, rust:clippy | PASS |
| census:clones | REVIEW (three twins, all already on main; none in this change) |
| census:concurrency, census:slop | PASS |
| rust:unit | PASS |
| rust:build, format-golden | PASS |
| e2e | PASS |

The first gate run failed one frozen TS split-call replay
(`every_captured_ts_split_call_replays_exactly`, call 75): its hand-edited
ledger's layout disagrees with its modules, which exp226 already treats
as "no usable layout". The name evidence read `nameToFiles` from that
same unusable layout. Fixed: names are read only when the per-statement
layout is usable (`layout_is_usable`, shared with `prior_hash_files`).
That guard cannot fire on a ledger the split writes, so the measured
binary (`humanify-ag-v2`, built before it) places the A/B hop the same.

## How it was measured

- Binaries: main `/work/final-3cb20421-frozen/target/release/humanify-ag`;
  candidate `/work/exp229-runs/bin/humanify-ag-v2` (this branch).
- Launch: the walk's `runs/2.1.239.toml` plus `llm.cache_dir` and
  `diagnostics.dump_artifacts`; script `/work/exp229-runs/ab/hop.sh`.
- Trees, classifier output, scripts: `/work/exp229-runs/ab/`,
  `/work/exp229-runs/ana/` (`s-main.json`, `s-mine2.json`, `bucket.py`,
  `alias.py`), matcher replay `/work/exp229-runs/replay/`.

## Not fixed, noted

- **An edited function renamed before placement.** The largest move left
  (380 lines on this hop) is an edited function that upstream moved into a
  new module. In the walk it kept its prior name; in this run the naming
  stage gave it a new one, so there is no name evidence. The function
  matcher's prior pairing (prior-match map) would be stronger evidence
  than the name.
- **Init names can rotate between modules** (three modules on the walk's
  hop: each init wears its neighbour's prior name, while the matcher's
  content pairing is right). That is a naming bug, and it is why init names
  are NOT used as module evidence here.
- **Inits are never refiled**, so a module whose init changed shape and
  whose other statements came from one prior file still moves when the
  carried statements are not more than half the module.
- `vendor/` <-> `src/` moves (69 lines) belong to the library placement.
- The extra export lines (+51) for code kept in its old file.

## What this cannot prove

- One hop. The walk's other hops and the split era were not run.
- Names come from the model; on another draw a different set of edited
  statements keeps its prior name, so the size of fix 2 varies per run.
- Keeping code in its old file can be wrong when upstream really moved it
  (the same trade as exp215 and exp226): the bundle does not say.
