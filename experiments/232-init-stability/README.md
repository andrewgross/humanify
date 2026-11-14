# exp232 — an init's name follows its module

STATUS: BUILT (TDD), GATED (all 11 stages pass, on `65017c701` and again
after the rebase onto `635dce3ca`), A/B MEASURED on one hop
(2.1.238 -> 2.1.239): a near-PINNED A/B on `635dce3ca` (exp233's
visible-prompt cache key; the candidate replayed main's cache and wrote
238 new entries of 5,833), plus four COLD legs on `65017c701` (two main,
two candidate). Boots (both halves). No eval, no walk. Branch
`exp232-init-stability`, rebased onto main `635dce3ca`. No Claude Code text is
committed; shapes are described generically.

## Totals

### Near-pinned A/B (base `635dce3ca`)

Main (built from `635dce3ca`) filled a fresh LLM cache; the candidate
(this branch rebased on it) replayed it. Cache files 5,833 -> 6,071: the
candidate wrote 238 entries (its 51 fewer module asks change the prompts
that show those wrappers' names).

| | main `635dce3ca` | **exp232** | change |
| --- | ---: | ---: | ---: |
| REAL (classifier) | 62,507 | 62,619 | +112 |
| noise | 4,027 | **3,823** | -204 |
| **noise per 100 real** | 6.44 | **6.11** | -0.33 |
| modules that kept their file but whose init name changed | 34 | **21** | -13 |
| noise lines naming such an init | 164 (0.26) | **106 (0.17)** | -58 |
| RENAME export-name, inside edited / unchanged statement | 294 / 134 | 258 / 118 | -36 / -16 |
| init export registrations counted as wiring for real code | 168 | 148 | -20 |
| module wrappers asked of the model | 212 | 161 | -51 |
| exit / `--version` / `-p boot-ok` | 0 | 0 / pass / pass | |

REAL +112 is inside the draw spread of REAL measured below (main legs
62,507 to 62,658 on this hop; candidate legs 62,619 to 62,646); the part
the change explains moves the other way (-20 init export registrations).

### Cold legs (base `65017c701`)

2.1.238 -> 2.1.239. Every leg starts from a copy of the walk tree
`walk-3cb20421-up/trees/2.1.238` with an empty LLM cache. Classifier: the
noise study's `tree_stmt.py`, grouped as in `rollup2.py` (upstream
re-nesting counts as REAL). Lines, then per 100 REAL lines.

| | main 1 | main 2 | **cand 1** | **cand 2** |
| --- | ---: | ---: | ---: | ---: |
| REAL (classifier) | 62,656 | 62,658 | 62,624 | 62,646 |
| noise | 3,912 | 3,847 | 3,811 | 3,738 |
| **noise per 100 real** | 6.24 | 6.14 | **6.09** | **5.97** |
| modules that kept their file but whose init name changed | 32 | 29 | **20** | **20** |
| noise lines naming such an init | 156 (0.25) | 144 (0.23) | **108 (0.17)** | **104 (0.17)** |
| noise lines naming an init that changed file | 10 | 10 | 10 | 10 |
| RENAME export-name, inside edited statement | 279 | 232 | 252 | 252 |
| RENAME export-name, unchanged statement | 132 | 132 | 126 | 118 |
| WIRING export: line renamed | 28 | 26 | 20 | 20 |
| module wrappers asked of the model | 212 | 212 | **161** | **161** |
| wrappers renamed to the prior module's init name (this step) | - | - | 55 | 55 |
| exit / cache files written | 0 / 5,817 | 0 / 5,822 | 0 / 5,824 | 0 / 5,868 |
| `--version` / `-p boot-ok` (cand 1) | | | pass / pass | |

- Noise per 100 real: main 6.24 / 6.14 (spread 0.10), candidate 6.09 /
  5.97. The mean moves -0.16; about half of that is the init-name lines
  themselves (-44 lines, 0.25 -> 0.17), the rest is inside the draw spread.
- REAL moves -12 to -34. 18 of it is explained: the export registration of
  an init whose name changed was counted as wiring for real code (the
  module is edited); with the name kept the line no longer changes
  (`wiring:export:for-real-code` lines naming an init: 164 / 162 -> 144 / 146).
- The step runs with the same counts in both candidate legs (4,419 pairs,
  4,363 already right, 55 renamed, 1 refused): it reads no model answer.

## What was wrong (measured before writing code)

On the exp229 tree for this hop (`exp229-runs/ab/mine2-2.1.239`, the
binary that became main `65017c701`), 35 modules kept their prior file
but their init's bundle name changed; 160 noise lines (0.26 per 100 real)
name one of them: every caller's `(0, alias.initX)()` line, the export
registration and its key. Only 8 inits changed file (10 lines of their own;
most of their lines are already counted above). The trail (`transfers.json`)
shows two causes:

1. **Rotation by the transfer.** The wrapper's arrow body is the module's
   setup code; with names masked, neighbouring modules' setup bodies share
   one shape, and the binding cascade handed three UNCHANGED modules each
   other's names (votes from their callers had it right, the cascade
   applied first).
2. **An edited setup body is asked anew.** When the setup body changed,
   nothing carries the wrapper, so the module step asks the model, which
   picks a new name although the module kept its file (and the reconcile
   then restores the file-local name, leaving only the bundle name, i.e.
   the export key and every caller, changed).

Placement already keeps those modules in their files; the name was the
only thing moving. The inits that changed file are upstream module
restructures (functions moving between modules) plus one new barrel that
took an edited barrel's file; not fixed here (see open items).

## The fix

`naming::module_names::init_identity` (called by `plan_module_naming`,
before it decides which wrappers to ask):

- The prior side is the prior split ledger's `fossilModules`: each
  module's sorted statement hashes and its last declared name (the init,
  which closes the segment). Read once by the CLI together with the exp211
  library carry (`PriorModuleCarry`); a ledger of another hash era lends
  nothing.
- The fresh side is the era's recorded modules, hashed with the split's own
  statement hash.
- A fresh and a prior module pair when each is the other's STRICTLY best
  match by multiset Jaccard and the score is >= 0.5. A tie pairs nothing;
  a prior `vendor/` module lends nothing. Offline on the ledgers, every
  pairing at 0.5 whose init name differed was a module the split had kept
  in its prior file.
- The paired wrapper takes the prior module's init name through the
  validated renamer (trail tier `module-identity`). A name another binding
  holds is refused unless that binding is itself being renamed away; a
  rotation is broken by moving one wrapper to its own minified name first.
- A wrapper renamed this way is settled, so the module step counts it as
  carried and never asks it.

## Tests (red, then green)

| test | before | after |
| --- | --- | --- |
| `an_edited_modules_wrapper_keeps_its_prior_init_name_by_content` | FAIL (wrapper asked of the model) | pass |
| `wrappers_wearing_each_others_names_take_their_own_modules_names` | FAIL | pass |
| `a_target_another_binding_holds_is_refused` | FAIL (no report) | pass |
| `init_identity_test` (6: pairing order-free, twins, mutual best, floor, ledger lends last declared name, stale era) | written after the code | pass |

The red runs were made by switching the step off (`carry_init_names`
returning early). A toy reproduction of the cascade's rotation was tried
and the transfer got it right there, so the rotation test hands the step
a ledger whose names are traded against the prior text instead.

## Gate

`cargo dev check` in the worktree: ALL 11 STAGES PASSED. census:clones
REVIEW lists only groups already on main.

## How it was measured

- Binaries: cold legs main `/work/exp232-runs/bin/humanify-ag-main` (built
  from `65017c701`), candidate `/work/exp232-runs/bin/humanify-ag-v1`;
  pinned legs main `humanify-ag-main635` (`635dce3ca`), candidate
  `humanify-ag-v2` (this branch rebased), same folder.
- Launch: the walk's `runs/2.1.239.toml` plus `llm.cache_dir` (a fresh,
  empty one per leg); script `/work/exp232-runs/ab/hop.sh`.
- Trees and classifier output: `/work/exp232-runs/ab/`, `/work/exp232-runs/ana/`
  (`a-<leg>.json`: `tree_stmt.py` plus every non-real line; `roll.py`
  totals and the init buckets; `pairsim*.py` the offline pairing
  simulation; `initrename.py` the per-module list).

## Not fixed, noted

- 20 modules per candidate leg still change their init name while keeping
  their file (cand 1): 12 pair below 0.5 (heavily edited modules); 4 share
  no statement with the prior module (barrels, and an upstream
  restructure where the module that took the file is another one); one
  has a twin by content (a tie); one refusal (an upstream module split:
  the init's name went with its setup body to the other half, which holds
  it); 2 read 0.5 on the placed trees but do not pair at naming time (one
  is the edited barrel whose file a new barrel took).
- Inits that change file (about 10 lines a hop): upstream restructures,
  and a new barrel taking an edited barrel's prior file because the
  fossil matcher sees only the shared setup shape. A placement tier that
  pairs modules by a unique init name would keep name and file together;
  not built.
- One hop. Cold, the candidate's -0.16 per 100 is near the main legs'
  spread (0.10); near-pinned it is -0.33. The init-name bucket's move
  (0.26 -> 0.17 in both protocols) is the mechanism's own.

## What this cannot prove

- The other hops and the split era were not run.
- Keeping a prior name for an edited module is right when the module is
  the same; at Jaccard 0.5 an upstream rename of the module's purpose
  keeps the old name (the file keeps its old name anyway).
