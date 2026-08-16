# 078 — file and folder names that survive a release

> **This is a PLAN, for review before any code.** Written 2026-08-16 after
> exp076 leg 3 measured the fossil layout as WORSE than what ships, and
> identified naming as the reason. Andrew: "let's make a plan for the
> file/folder naming improvements so we can review it, including everything
> needed to run, test, and eval it, so a sub agent can run it."

## The measured problem

exp076 ran three legs of a real version walk — one cold start then each
release inheriting the last, four consecutive versions, scored on the two
warm pairs. On a busy release:

|                                 | shipped layout | fossil layout |
| ------------------------------- | -------------: | ------------: |
| diff lines                      |     **23,323** |        33,297 |
| files added / removed / renamed |      0 / 0 / 0 |  67 / 37 / 23 |
| file add-remove lines           |          **0** |        12,902 |
| name-only noise                 |        **136** |         1,202 |

The fossil layout emits one file per real module, which is the point — but a
file whose NAME changes reads to git as a whole file deleted plus a whole
file added. Same code, counted twice, zero real change. At ~190 lines/file
the 23 renames alone are ~4,400 lines.

Placement is not the problem: a calm release under the fossil layout moves
**2 statements** between files. Naming is the problem.

## FOUR mechanisms, not one

Read off the actual renames in the walk trees (`/work/walk-anchor-off/`):

1. **File stem = ONE identifier.** `moduleStem` takes the module's first
   hoisted function/class name. The naming stage is an LLM call and exp052
   measured two cold legs disagreeing on 33.4% of decisions by a different
   word — so a third of fresh-minted files can rename on any release.
   `src/record-hook-activity.js` → `src/is-trigger-allowed.js`.
2. **FOLDER names are the same stem, and cost more.** Folders are named
   `src/<stem>` from an anchor or barrel module. One drifted identifier
   renames the folder and moves EVERY file in it.
   `src/filter-active-prompt-commands/…` → `src/get-unique-prompts/…`,
   `src/has-properties/render-mermaid.js` → `src/get-action-list/…`.
   This is the biggest of the four by files affected and the one the
   "first method changed" framing misses.
3. **Collision suffixes are discovery-order dependent.** `claimPath` appends
   `-2`, `-3` in walk order, so two same-stem modules can swap suffixes
   between releases. `src/load-image-processor-2.js`,
   `src/is-rule-or-all-subcommand-results-2.js`.
4. **Only MATCHED modules carry a name at all.** A matched module inherits
   its path verbatim and is safe by construction. Everything above applies
   only to the unmatched remainder (219 of 3,273 on 85→86, 7.4% of
   statement mass) — so the population is small and the cost per member is
   high.

## Task 0 — ATTRIBUTE THE COST BEFORE WRITING CODE (mandatory)

The four mechanisms above are read off examples. Their SHARES are not
measured, and this project has published a wrong number by sizing a lever
from its largest single example before (measurement-pitfalls rule 4: 83× →
1.0×). Do this first; if the distribution is not what the plan assumes, the
plan changes.

**Instrument:** offline, on trees already on disk — no pipeline run, no LLM.
`/work/walk-anchor-off/2.1.215/src` and `/work/walk-anchor-off/2.1.216/src`,
plus both `.humanify/split-ledger.json`.

**Produce, for the 215→216 hop:**

| class                               | files | diff lines |
| ----------------------------------- | ----: | ---------: |
| folder renamed, file stem identical |     ? |          ? |
| file stem changed, folder identical |     ? |          ? |
| both changed                        |     ? |          ? |
| collision suffix changed only       |     ? |          ? |
| genuinely new / deleted module      |     ? |          ? |

**Gate on it:** if "genuinely new/deleted" dominates, most of the 12,902
lines are honest and the whole plan is mis-aimed — say so and stop. Write
the finding either way.

## The design

One principle: **a name is a carried asset keyed to module identity, minted
once from broad evidence and never re-derived.** The path carry already does
this for matched modules; every task below extends it to the rest.

### Task 1 — name from breadth, not from position (deterministic, no LLM)

Replace "first hoisted declaration" with a rank over ALL the module's
declared symbols, so one drifted identifier cannot rename the file.

- Rank by **reference count within the bundle** (a structural property),
  tie-break by declaration order. Measured input: 60.6% of modules declare
  ≤1 function/class, but only 15.7% declare ≤1 SYMBOL — so rank over all
  declared symbols, not just functions (exp076 `naming-evidence.ts`).
- Andrew's rule, kept: **a module with exactly one exported function keeps
  that function's name.** No LLM, no ranking, nothing to drift.
- This is free and strictly more stable than position. Do it first and
  measure it alone.

### Task 2 — carry the name through weaker identity than the path

A module the matcher declines currently mints a whole fresh identity. Give
the NAME a cheaper carry than the PATH:

- If a prior module shares this module's **stem and ≥1 import edge**, or its
  stem is unique among unmatched modules on BOTH sides, reuse its NAME
  without claiming its path.
- This is the question exp074 left open, narrowed: reusing a name is a much
  weaker commitment than inheriting a path, so the evidence bar can be lower.
  Andrew's call on the rule, but the narrowing may make it moot.

### Task 3 — folder names carried, not re-derived

The largest mechanism. A folder's name must not depend on one member's stem.

- Name a folder from its **stable member set**: if a folder in the prior
  tree shares ≥50% of its members (by module identity) with a folder being
  formed now, it keeps the prior name.
- Only then fall back to a stem-derived name, and derive that stem by the
  Task 1 rank so it is itself stable.

### Task 4 — collision suffixes from content, not order

`claimPath` appends `-2` in discovery order. Derive the disambiguator from
the module's content hash instead (`foo-a3f2.js`), so two same-stem modules
cannot swap suffixes between releases. Ugly names are better than swapping
names; if that is unacceptable, keep ordinals but assign them by content
hash order, which is stable.

### Task 5 — LLM naming, LAST and only if Tasks 1–4 leave a gap

Andrew, 2026-08-15: "we can have an llm pick the file name based on a
sampling of the exported function names etc, and can re use that between
versions." Cost is explicitly not a constraint.

- Sample **all declared symbols**, not just exported functions.
- Ask ONCE per module identity; store the answer in the ledger; never
  re-ask for a module that carries a name.
- Deterministic fallback = Task 1's rank, so an LLM failure degrades to
  something stable rather than to nothing.
- **Sequenced last on purpose:** the LLM is a source of variance (33.4%
  disagreement between cold legs), so it must sit BEHIND the carry, not in
  front of it. If Tasks 1–4 land the renames near zero, this becomes a
  quality change with no stability claim, which is a much easier thing to
  judge.

## How to run it

Every task: red/green TDD, tests colocated as `*.test.ts`, and
**`npm run check` green (8 stages) before every commit**. Biome's complexity
ceiling is stricter pre-commit than in the gate — run `npx biome check <file>`
first.

## How to evaluate it

**The eval harness is the WRONG instrument here and must not be used for the
verdict.** Its four pairs are all big-change releases, its base is rebuilt
against a pre-fossil prior, and exp076 spent two days arguing over numbers
that came from it. Use the walk.

```bash
# three legs, ~2h each, sequential (each hop peaks near 25 GB)
experiments/076-statement-placement/walk.sh /work/<label>-new
experiments/076-statement-placement/walk.sh /work/<label>-old --disable fossil-split
```

Freeze the tree first (`git worktree add --detach /work/<label> <sha>`) —
a run against a live agent worktree once executed half-edited code.

**Success criteria, fixed NOW, before any code:**

| criterion                  | target                                      |
| -------------------------- | ------------------------------------------- |
| files renamed on 215→216   | **23 → ≤ 5**                                |
| file add-remove lines      | 12,902 → **< 6,000**                        |
| total diff lines, busy hop | 33,297 → **≤ 23,323** (parity with shipped) |
| statements moved file      | must not exceed 161                         |
| calm hop (214→215)         | must not exceed 2,084 lines                 |
| `novel` / `realLn`         | byte-identical to the control leg           |

**Parity with the shipped layout on the busy hop is the bar for merging the
whole fossil stack.** Below parity it is not worth one file per module;
at or above, the stack merges and exp077's folder grouping follows.

## What would refute this plan

- Task 0 shows most of the 12,902 lines are genuinely new/deleted modules —
  then naming is not the lever and the stack's cost is structural.
- Tasks 1–4 land and renames barely move — then the renames are driven by
  match FAILURES, not by naming, and the work belongs in the matcher.
- Anything here moves `novel`/`realLn`. Those have a measured band of zero;
  any movement is real change lost, which is the one failure that outranks
  every line-count win.

## Reading order for whoever picks this up

1. `experiments/076-statement-placement/README.md` — the three-leg result
   and why the eval cannot settle this.
2. `docs/measurement-pitfalls.md` — eleven rules, seven learned by
   publishing a wrong number first.
3. `experiments/077-flat-root-grouping/README.md` — the folder-grouping work
   this unblocks, with its own sizing already done.
