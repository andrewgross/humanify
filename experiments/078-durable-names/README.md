# 078 — enclosure identity first, names as labels

> **This is a PLAN, for review before any code.** Rewritten 2026-08-16 after
> Andrew corrected its framing:
>
> > "really we should say 'everything in this enclosure is a single file' →
> > have some way to uniquely identify this whole bundle → unminify stuff →
> > pick a name for the file of the bundle → always rematch based on the
> > bundle, not the file name, and just re-use the file name for the bundle
> > so it doesn't move in git"
>
> That is the right architecture, and the first version of this plan had it
> backwards: it treated name stability as the goal, when the name is a LABEL
> and IDENTITY is the thing that has to be stable.

## Where the pipeline already agrees with that model, and where it leaks

| step                                       | state                                                                                                                      |
| ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| "everything in this enclosure is one file" | **done** — read off the bundle's `__esm` segments (exp068/070), 99.98% statement attribution, byte-stable across cold runs |
| unminify                                   | done                                                                                                                       |
| pick a name for the enclosure              | done, but from ONE identifier                                                                                              |
| rematch by enclosure, reuse its name       | **done for 93.3% of modules** — a matched module inherits its prior path verbatim and cannot move                          |
| identity must not depend on the name       | **VIOLATED**                                                                                                               |

The architecture is already the one described. It leaks in exactly two
places, and both are measurable.

### Leak 1 — identity reads the name (circular)

`matchFossilModules` tier C compares a **stem** on both sides. The prior
side's stem is `priorFileStem(m.file)` — the file's NAME. The fresh side's is
`moduleStem(...)` — the kebab of the module's first hoisted declaration,
an identifier the LLM named this run.

So a module's identity partly depends on what the model decided to call one
of its functions, and the name it then inherits depends on that identity.
Name → identity → name. exp052 measured the model disagreeing with itself on
33.4% of decisions across two cold legs, so this is a live coin-flip.

Tier C is not junk — exp074 added it because it holds 3,204 churned require
lines on 85→86, and it must not simply be deleted. But it is identity
evidence of the wrong KIND, and the plan replaces the evidence, not the tier.

### Leak 2 — identity is content-first, so a rewritten enclosure loses it

Tier B requires content overlap ≥ 0.5 **before** import-edge agreement is
considered at all. An enclosure that kept its position in the graph but
rewrote half its body is never a candidate, so it mints a fresh identity and
its file appears in git as a delete plus an add.

Measured (exp074): `redact-url` at 0.42 overlap and `manage-marketplaces` at
0.33 — each with exactly ONE candidate on each side, both plainly the same
enclosure, both declined.

That is backwards for the model Andrew describes. An enclosure is "the thing
these 12 files import and which imports these 3". Its body is what CHANGES
between releases; its position is what persists.

## The measured problem this fixes

exp076 leg 3, real version walk, busy release (2.1.215 → 2.1.216):

|                                 | shipped layout | fossil layout |
| ------------------------------- | -------------: | ------------: |
| diff lines                      |     **23,323** |        33,297 |
| files added / removed / renamed |      0 / 0 / 0 |  67 / 37 / 23 |
| file add-remove lines           |          **0** |        12,902 |

Placement is already nearly perfect — a calm release moves **2 statements**
between files. The 12,902 lines are enclosures losing their identity and
being re-minted.

## Task 0 — WHICH LEAK, AND HOW BIG (mandatory, gates everything)

**The one question:** of the enclosures that minted a fresh identity, how
many actually existed in the prior release?

A genuinely new module SHOULD get a new file — correct behaviour, honest
diff. The cost is only the ones that existed and were not recognised.

**Instrument:** offline, on trees already on disk. No pipeline run, no LLM.
`/work/walk-anchor-off/2.1.215/` and `/work/walk-anchor-off/2.1.216/`
(`src/` plus `.humanify/split-ledger.json` for each).

**Produce:**

| class                                        | modules | diff lines |
| -------------------------------------------- | ------: | ---------: |
| genuinely new / deleted enclosure            |       ? |          ? |
| existed in prior, matched, name held         |       ? |          ? |
| existed in prior, DECLINED by the matcher    |       ? |          ? |
| …of those, declined on content overlap alone |       ? |          ? |
| …of those, recoverable by graph position?    |       ? |          ? |

"Existed in prior" must be established WITHOUT the production matcher and
WITHOUT names — use importer/importee sets mapped through the confidently
matched majority. Ground truth has to be independent of the thing under
test: exp076 already retracted a measurement whose identity floor was set at
the matcher's own threshold and which therefore reported a confident zero.

**Gate:** if genuinely-new dominates, the 12,902 lines are honest, one file
per module is structurally more expensive than the shipped layout, and that
is the finding — write it and stop. If declined-but-present dominates,
proceed; the last row sizes Task 2 before it is built.

## Task 0 — EXECUTED 2026-08-17. Gate PASSED, and the plan is re-ordered.

`task0-attribute.ts`, offline on the exp076 walk trees (2.1.215 → 2.1.216),
no pipeline run and no LLM. Ground truth from 3,863 unique-signature seed
pairs, then importer/importee agreement mapped through those seeds — never
the tiers under test, never a name.

| class                                   | enclosures |   lines | median overlap |
| --------------------------------------- | ---------: | ------: | -------------: |
| matched, name held                      |  **4,735** | 849,500 |              — |
| **EXISTED in prior, DECLINED**          |     **74** |  16,432 |       **0.30** |
| …of those, below EVERY content floor    |         74 |       — |              — |
| existed but ambiguous (tied candidates) |         24 |   3,548 |           0.00 |
| **genuinely new**                       |     **17** |   3,926 |              — |

Split by whether the path actually moved, because a re-mint only costs git
lines when it does:

|                                                      | enclosures |      lines |
| ---------------------------------------------------- | ---------: | ---------: |
| re-minted to the SAME path (free — reads as an edit) |         40 |     12,466 |
| **re-minted to a MOVED path (add + delete)**         |     **58** | **17,781** |

**Verdict: 81.9% of the added-file mass belongs to enclosures that EXISTED
and were re-minted. Only 17 files, 3,926 lines, are genuinely new.** The plan
is correctly aimed and Task 0's gate is passed.

### What Task 0 changes about the plan

1. **Task 2 is the lever, not Task 1.** Every one of the 74 declined
   enclosures sits BELOW 0.5 content overlap — median 0.30. No content tier
   can reach them: tier B's floor is 0.5 and tier C's is 0.7. The refusal is
   structural, not a threshold that could be nudged. Graph-position identity
   is the whole win.
2. **Tier C is smaller than feared HERE.** It contributed 36 of 4,735 matches
   on this pair (against 44 pairs / 3,204 lines on 85→86). The circularity is
   still wrong and still worth removing, but it is hygiene, not the lever —
   and removing it must not lose those 36.
3. **The match rate is better than previously quoted.** 4,735 of 4,850 =
   **97.6%**, not the 93.3% measured on minified input where the stem tier is
   inert. The unmatched population is 115 enclosures, and 98 of them existed.
4. **24 ambiguous cases need a rule, not a threshold.** Tied candidates at
   overlap 0.00 — pairing one arbitrarily would carry a name onto unrelated
   code, which is worse than minting. They stay unmatched by design.

**Build order therefore: Task 2 first** (additive, a new tier after the
content tiers), then Task 1 reuses its edge machinery to replace tier C's
evidence — at which point tier C may not be needed at all, which is a
cleaner outcome than editing it in place.

### One number NOT to quote

The 17,781 here and exp076's 12,902 `fileAddRemove` are different measures:
this counts whole files on both sides of a re-mint, the eval counts what its
differ attributes to added/removed files. They agree in order of magnitude
and direction; they are not the same quantity and the smaller one is the one
a reviewer sees.

## The design

### Task 1 — identity stops reading names

Replace tier C's stem with evidence of the same shape from a stable source:
the enclosure's **importer and importee sets mapped through already-made
matches**. Two enclosures imported by the same files, and importing the same
files, are the same enclosure regardless of what anything is called.

- Must hold tier C's 3,204 lines on 85→86 (exp074's measured win), or the
  replacement is a regression wearing a better argument.
- `moduleStem` stops feeding `matchFossilModules` entirely. After this, no
  identity decision anywhere reads an LLM-produced name.

### Task 2 — identity survives a rewritten body

Let graph position carry identity alone when content cannot.

- If an enclosure has exactly ONE plausible counterpart by importer/importee
  agreement and that counterpart is unclaimed, pair them **at any content
  overlap** — process of elimination, nothing else can claim it.
- Ordered AFTER the content tiers, so it only ever sees leftovers.
- Sized by Task 0's last row before it is written.
- Refutation condition, stated now: if it pairs enclosures that are not the
  same thing, a carried name lands on changed code, which is worse than
  minting. Task 0's independent pairing is the check.

### Task 3 — the name becomes a stored label, not a derived one

Today a matched module inherits its PATH, carrying the name implicitly.
Make it explicit so the name can survive things the path cannot.

- Record `name` per enclosure in the split ledger beside `hashes`/`imports`.
- On a match the name is READ from the ledger, never recomputed.
- Minting happens only for enclosures with no counterpart — the only place
  naming quality matters at all.
- Also makes folder renames cheap to fix: a folder keeps its name when its
  member set is mostly unchanged, because members now have stable identities
  to compare.

### Task 4 — mint from breadth, and stably

Reached only by genuinely new enclosures, after Tasks 1–3.

- Rank ALL declared symbols by reference count, tie-break by declaration
  order — not "whichever came first". 60.6% of modules declare ≤1
  function/class but only 15.7% declare ≤1 SYMBOL (exp076
  `naming-evidence.ts`), so rank over symbols, not functions.
- **Andrew's rule, kept:** an enclosure with exactly one exported function
  keeps that function's name.
- Collision suffixes derive from the content hash, not discovery order, so
  two same-stem enclosures cannot swap `-2` between releases.

### Task 5 — LLM naming, last

Andrew, 2026-08-15: "we can have an llm pick the file name based on a
sampling of the exported function names etc, and can re use that between
versions." Cost is explicitly not a constraint.

- Sample ALL declared symbols, not just exported functions.
- Asked ONCE per enclosure identity, stored by Task 3, never re-asked.
- Deterministic fallback is Task 4, so a model failure degrades to something
  stable rather than to nothing.
- Last on purpose: after Tasks 1–3 the model cannot influence identity, so
  this becomes a pure quality change with no stability claim — the only
  order in which its 33.4% disagreement rate is harmless.

## How to run it

Every task: red/green TDD, tests colocated as `*.test.ts`, `npm run check`
green (8 stages) before every commit. Biome's complexity ceiling is stricter
pre-commit than in the gate — run `npx biome check <file>` first.

Tasks 1 and 2 change matching, so each needs its own kill switch for the A/B
plus an entry in `SWITCH_KIND` (`scripts/switch-census.ts`), or the gate
fails.

## How to evaluate it

**The eval harness must NOT decide this.** Its four pairs are all big-change
releases and its base is rebuilt against a pre-fossil prior; exp076 spent two
days on numbers from it and the walk overturned them. Use the walk.

```bash
git worktree add --detach /work/<label> <sha>     # freeze first
cp -al /work/exp074-frozen/node_modules /work/<label>/node_modules
experiments/076-statement-placement/walk.sh /work/<label>-new
experiments/076-statement-placement/walk.sh /work/<label>-old --disable fossil-split
```

~2h per leg (≈70 min cold start, ~15 min per warm hop), sequential — each hop
peaks near 25 GB, and parallel legs would contend for the model server too.
Scores 2.1.214→215 (calm) and 2.1.215→216 (busy).

**Success criteria, fixed NOW:**

| criterion                                      | target                            |
| ---------------------------------------------- | --------------------------------- |
| enclosures re-minted despite existing in prior | → near zero                       |
| files renamed, busy hop                        | 23 → **≤ 5**                      |
| file add-remove lines, busy hop                | 12,902 → **< 6,000**              |
| total diff lines, busy hop                     | 33,297 → **≤ 23,323**             |
| statements moved file, busy hop                | must not exceed 161               |
| calm hop total                                 | must not exceed 2,084 lines       |
| `novel` / `realLn`                             | byte-identical to the control leg |

**Parity with the shipped layout on the busy hop is the merge bar for the
whole fossil stack** (exp070/073/074/075/076). Below it, one file per module
is not worth having.

## Task 2 VALIDATED on a real walk (2026-08-17)

One leg at `bd90924`, four consecutive versions, cold start then each hop
inheriting the last. The control is exp076's `walk-anchor-off` leg, which
differs from this build by **exactly tier D** — so no second leg was needed.

|                                  |               shipped layout |    fossil | fossil + tier D |
| -------------------------------- | ---------------------------: | --------: | --------------: |
| **calm hop** (214→215) churn     |                        1,673 |     2,084 |       **1,621** |
| calm statements moved file       |                            0 |         2 |           **0** |
| calm name-only noise             |                           94 |       164 |          **10** |
| **busy hop** (215→216) churn     |                   **23,323** |    33,297 |          27,646 |
| busy statements moved file       |                            1 |       161 |             107 |
| busy file add/remove lines       |                            0 |    12,902 |       **5,469** |
| busy files added/removed/renamed |                        0/0/0 |  67/37/23 |      **35/5/3** |
| busy name-only noise             |                          136 |     1,202 |             994 |
| `novel` / `realLn`               | 146 / 33,135 · 986 / 122,066 | identical |   **identical** |

**The hold columns are byte-identical across all three legs.** No real change
was lost to buy any of this.

### On a CALM release the fossil layout now BEATS what ships

1,621 lines against 1,673, zero statements moved file, and one NINTH the
name-only noise (10 against 94). That is the first time one file per module
has been cheaper than the shipped layout on any measure, and calm releases
are most releases.

### Against the criteria fixed before the work

| criterion                | target       |     result |     |
| ------------------------ | ------------ | ---------: | --- |
| files renamed, busy hop  | ≤ 5          |      **3** | ✅  |
| file add-remove lines    | < 6,000      |  **5,469** | ✅  |
| statements moved file    | ≤ 161        |    **107** | ✅  |
| calm hop total           | ≤ 2,084      |  **1,621** | ✅  |
| `novel` / `realLn`       | identical    |  identical | ✅  |
| **total diff, busy hop** | **≤ 23,323** | **27,646** | ❌  |

Five of six. **Parity on the busy hop — the merge bar for the whole fossil
stack — is missed by 4,323 lines.**

### Where the remaining 4,323 lines are

Almost exactly the surviving file add/remove mass: 5,469 lines from 35 added
and 5 removed files, where the shipped layout has zero by construction (its
file set is fixed). Task 0 put genuinely-new enclosures at 17–24, so roughly
**11–18 of those 35 are still being re-minted** — and those are what Tasks 1,
3, 4 and 5 exist for.

The precision worry recorded before the run did not materialise in the
numbers that matter: `novel`/`realLn` held exactly, and every churn column
moved down. It remains true that ~36% of tier D's pairings disagree with the
independently-derived filename, and that is worth revisiting if a later task
makes names trustworthy enough to cross-check against.

## What would refute this plan

- Task 0 shows genuinely-new enclosures dominate → the cost is structural and
  neither identity nor naming is the lever.
- Tasks 1–3 land and renames barely move → something neither identity nor
  carry reaches is driving them, and the mechanism is still unfound.
- Task 2 pairs enclosures that are not the same thing → a carried name on
  changed code is worse than an honest new file.
- Anything moves `novel`/`realLn`. Measured band is zero; any movement is
  real change lost, which outranks every line-count win.

## Reading order

1. `experiments/076-statement-placement/README.md` — the three-leg walk
   result and why the eval could not settle it.
2. `docs/measurement-pitfalls.md` — eleven rules, seven learned by publishing
   a wrong number first.
3. `experiments/077-flat-root-grouping/README.md` — the folder grouping this
   unblocks, already sized.
