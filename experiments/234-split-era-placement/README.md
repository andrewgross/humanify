# exp234 — split era placement: a carved-out module joins the file its code came from

STATUS: BUILT (TDD), GATED (all 11 stages pass), MEASURED on five real
hops with the live model: one main-only hop on the split era
(2.1.281 -> 2.1.282, the step-1 read), one near-pinned split-era A/B
(2.1.282 -> 2.1.283, both legs from main's own 2.1.282) and one
near-pinned pre-split check (2.1.238 -> 2.1.239). In both A/Bs the
candidate replayed main's cache and wrote 0 entries. Boots (both halves)
on both candidate trees. No eval, no walk. Branch
`exp234-split-era-placement` from main `38bda6d1d`. No Claude Code text,
file name or chunk name is committed; shapes are described generically.

## Totals

Classifier: the noise study's `tree_stmt.py`, grouped as in `rollup2.py`
(upstream re-nesting counts as REAL). Lines, then per 100 REAL lines.

### Split-era A/B, 2.1.282 -> 2.1.283 (steady state)

Both legs start from main's own 2.1.282 (written by `38bda6d1d` from a copy
of the walk's 2.1.281), so neither leg is a transition. Main filled a fresh
LLM cache (5,393 entries); the candidate replayed it: 5,393 -> 5,393, 0
written.

| bucket | main `38bda6d1d` | **exp234** | change |
| --- | ---: | ---: | ---: |
| REAL (classifier) | 94,186 | 92,091 | -2,095 (explained below) |
| noise | 16,730 | **14,938** | -1,792 |
| **noise per 100 real** | 17.76 | **16.22** | -1.54 |
| placement family (rows below) | 9,670 (10.27) | **7,686 (8.35)** | -1,984 |
| WIRING require: added/removed for unchanged code | 3,034 (3.22) | **1,921 (2.09)** | -1,113 |
| MOVE across files | 2,147 (2.28) | **1,801 (1.96)** | -346 |
| WIRING require: line renamed | 1,838 (1.95) | **1,442 (1.57)** | -396 |
| RENAME module-alias, inside edited statement | 1,070 | 1,014 | -56 |
| RENAME module-alias, unchanged statement | 688 | 662 | -26 |
| RENAME module-alias, unchanged, moved file | 178 | 178 | 0 |
| WIRING export: added/removed for unchanged code | 375 | 344 | -31 |
| WIRING export: line renamed / moved | 264 / 76 | 256 / 68 | -8 / -8 |
| RENAME local, unchanged statement (not placement) | 354 | 504 | +150 |
| whole tree, changed lines | 110,916 | 107,029 | -3,887 |
| exit / `--version` / `-p boot-ok` | 0 | **0 / pass / pass** | |

REAL -2,095, every line accounted for:

- -1,274 `require` lines that served new code (`wiring:require:for-real-code`).
  An importer that calls the new module's init no longer needs a new
  `require` of a new file: the init now sits in a file it already requires.
- -821 net in statement-level REAL (`real:new` -1,552, `edited:*` +554,
  `real:gone` +188). The carved module's new statements now sit in the same
  file as the prior code they resemble, so the classifier pairs them as
  edited and counts only their changed lines, where before the whole
  statement was new in a new file. Exactly the files the change re-homed
  carry the difference (e.g. two fresh files with 559 and 506 `real:new`
  lines on main are gone on the candidate).
- +150 local renames: one file now holds a re-homed module's statements
  next to a prior statement of the same shape; the classifier pairs them.

### Pre-split check, 2.1.238 -> 2.1.239

Both legs from a copy of the walk tree `walk-3cb20421-up/trees/2.1.238`.
Main filled the cache (5,820 entries); the candidate replayed it, 0 written.

| | main `38bda6d1d` | **exp234** |
| --- | ---: | ---: |
| REAL | 62,629 | 62,623 (-6) |
| noise | 3,753 | 3,749 |
| **noise per 100 real** | 5.99 | **5.99** |
| every placement bucket | | equal (require -2, between -2) |
| exit / `--version` / `-p boot-ok` | 0 | 0 / pass / pass |

Noise did not rise and REAL did not move: pre-split bundles carve few
modules, and the rule only fires on those.

## Step 1: what main `38bda6d1d` already removes on a split-era hop

2.1.281 -> 2.1.282, prior = a copy of the walk's 2.1.281 (written by
`3cb20421`, which lacks exp229 / exp232 / exp233).

| | walk #5 (`3cb20421`) | main `38bda6d1d` |
| --- | ---: | ---: |
| REAL | 135,728 | 128,190 |
| noise | 41,254 | 25,375 |
| **noise per 100 real** | **30.39** | **19.79** |
| MOVE across files | 11,110 (8.19) | **1,073 (0.84)** |
| WIRING require for unchanged code | 3,673 | 3,338 |
| RENAME module-alias inside edited | 3,761 | 2,692 |

exp229 already removes nearly all of the 8.19 MOVE share: 10,159 of the
walk's 13,116 moved lines were EDITED functions whose module moved (the
two largest: 1,622 moved lines for a function carved into a new module,
1,326 for one that left the one giant file); their names now keep them in
their prior file. Some
of the difference is a transition: the prior has no exp232 init names, so
main also renames inits on this hop (REAL -7,538 is partly that, partly
exp229 diffing moved edited functions in place).

## Step 2: why statements still change file, and the knock-on lines

On the main legs, the placement family is 9,670 lines on 2.1.282 -> 283
(10.27 per 100) and about 9,700 on 2.1.281 -> 282 (7.6). By cause:

| cause | 281 -> 282 | 282 -> 283 | fixed here |
| --- | ---: | ---: | --- |
| A. **a new module carved out of prior modules**: upstream made a new chunk (new init) from code that lived in other modules. exp226/229 keep each carried statement in its prior file, so the new file holds only the init (and any new code), and every importer of the init adds a `require` of it, often removing another | `require` lines to new files with 20+ importers: 1,214 | 1,721 | **yes (fix 1)** |
| B. **a vendor file named by its content, renamed by an edit**: an unindexed library module is filed `vendor/<pkg>/module-<content key>.js`; one edit renamed it, and ~795 importers changed `require` and alias | **4,947 lines** (1,530 require + 3,398 alias) | 0 | **yes (fix 2)**, not measured on a hop |
| C. module inits that change file (inits are never refiled; the module matched or kept another file) | 932 MOVE lines | 1,617 | no |
| D. edited functions whose module moved with no hash or name evidence | 397 | 1,211 | no |
| E. the chunk graph itself changed: an importer's list of init calls lost or gained a shared chunk (each such edge is one init-call line plus one `require`) | most of the rest of the require churn | | no (upstream) |
| F. export registrations following a moved module | 570 | 198 | no |
| unchanged statements still moving (non-unique hash, vendor <-> src) | 76 | 324 | no |

(Moved-line counts are both sides of the cross-file statement pairs.)
Neither of the two shapes named in the brief is still large on main: the
one giant file loses or gains 50 to 210 moved lines a hop (edited
functions), and "a block carved out into a new file" is now cause A, whose
statements already stay put; what still costs is the new init's file.

Measured before writing code (`would.py`): on 282 -> 283, of the 1,721
importer `require` lines pointing at new files with 20+ importers, 1,264
would vanish if the new module lived in the file most of its carried
statements came from (the importers already require that file). On the
candidate leg: 621 left.

## The fixes

1. **A carved module joins the majority file** (`fossil.rs`,
   `kept_prior_file`). exp215 let an UNMATCHED module keep a prior file only
   when every carried statement named that one file. Now, when the carried
   statements are still more than half the module but disagree, the module
   keeps the prior file that more than half of them name, if that file is
   an app file (never `vendor/`, never the entry file). Its carried
   statements stay where they were either way (exp226); the rule decides
   only where the init and the new statements go. A carved module whose
   carried code splits evenly, or a mostly new module, is still named
   fresh.
2. **A content-named vendor file keeps its prior name** (`fossil.rs`,
   `carried_content_name`; predicate `library_layout::is_synthetic_vendor_path`).
   When the library placement gives a module a synthetic name
   (`module-<8 hex>[-n].js`), the module matcher pairs it with a prior
   module whose file is also a synthetic name in the SAME package folder,
   and no other library module of this run claims that path, the module
   keeps the prior path. An index-composed path always wins.

Neither fix reads anything Bun-specific: both run in the marker split
(any bundle whose lazy-init module markers describe it: Bun single-file,
Bun code-split after the chunk link, esbuild) on the ledger the split
writes, and fix 2 on names mode's vendor paths. They are NOT gated on the
input layout; the e2e fixtures (webpack, browserify, esbuild, rollup, Bun)
pass unchanged, and the pre-split hop shows no change.

## Tests (red, then green)

| test | before | after |
| --- | --- | --- |
| `a_module_carved_mostly_from_one_prior_file_takes_that_file` | FAIL (fresh file) | pass |
| `a_carved_module_holds_its_file_on_the_next_hop` | guard | pass |
| `a_carved_module_whose_carried_statements_split_evenly_is_named_fresh` | guard | pass |
| `a_carved_module_never_joins_a_vendor_file_by_majority` | guard (fails without the app-file check) | pass |
| `a_module_whose_carried_statements_came_from_two_prior_files_takes_the_majority_file` (was `..._is_named_fresh`, exp226) | asserted the old rule | pass |
| `an_edited_library_module_keeps_its_prior_content_named_file` | FAIL (new content name) | pass |
| `a_library_module_never_takes_a_prior_name_from_another_package_or_an_index_file` | guard | pass |
| `a_prior_content_name_another_library_module_claims_is_not_carried` | guard | pass |
| `a_synthetic_vendor_name_is_recognised_and_an_index_name_is_not` | new | pass |

## Gate

`cargo dev check` on the final code: all 11 stages pass.

| stage | result |
| --- | --- |
| shims, no-private-addresses, rust:fmt, rust:clippy | PASS |
| census:clones | REVIEW (five twins, all already on main; none in this change) |
| census:concurrency, census:slop | PASS |
| rust:unit | PASS |
| rust:build, format-golden | PASS |
| e2e | PASS |

## How it was measured

- Binaries: main `38bda6d1d` (built from that commit, clean tree);
  candidate built from this branch.
- Launch: the walk's `runs/<v>.toml` plus `llm.cache_dir`; script
  `/work/exp234-runs/ab/hop.sh`. Trees, caches and logs in
  `/work/exp234-runs/ab/`; classifier outputs and analysis scripts
  (`tsd.py` = `tree_stmt.py` plus a per-move dump, `attr.py`, `edges.py`,
  `would.py`, `mvwhy.py`, `modcmp.py`) in `/work/exp234-runs/ana/`.

## Not fixed, noted

- **Fix 2 is not measured on a hop** (the lane's five hops went to step 1,
  the split-era A/B and the pre-split check). Its size on 281 -> 282 is
  read off main's leg: 4,947 noise lines (3.86 per 100 real) name the
  renamed vendor file. It fired on 1 of 6 walk hops checked.
- **Inits never move** (cause C, the largest MOVE left): a module whose
  init carries the prior init's name but whose module was matched or kept
  elsewhere moves its init, and with it every importer's `require`.
- **Upstream chunk-graph churn** (cause E): when the bundler regroups
  shared chunks, hundreds of chunks change their init-call lists at once;
  each lost or gained edge is an init call plus a `require`.
- Edited functions with no evidence (cause D), export registrations (F).

## What this cannot prove

- One steady-state split-era hop and one pre-split hop. Other hops carve
  modules differently.
- Keeping a carved module in its majority file is wrong when upstream
  really made a new source file; the bundle does not say.
