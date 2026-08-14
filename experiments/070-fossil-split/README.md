# 070 — fossil-guided splitting: emit the file layout the bundle records

> **This is a BRIEF — a hypothesis, including its cautions.**
>
> exp068 verified (against known source, answer-key method) that a Bun
> bundle records its original file structure: one `__esm` lazy init per
> source file, the init's write-set = that file's contents, leading
> init-calls = its imports, 99.98% statement attribution, byte-stable
> across cold runs. The bundle for 2.1.86 records **3,273 modules; we
> emit 1,497 files** — 2.2× coarser. exp069 closed the ask-side road:
> **69% of remaining hidden churn is DERIVED** (importer lines,
> aliases, export keys) — downstream of layout. This experiment makes
> the split consume the fossil map. It is the largest change of the
> campaign and its one-time relayout churn is explicitly a NON-COST
> (Andrew, 2026-08-13): judge steady state only.

## Plugin architecture (Andrew, 2026-08-14 — a design requirement)

Bun-specific by declaration, expressed in the registries:

- The **bun unpack adapter** (stage 2/3 registry) grows the fossil
  extraction: its unpack output carries an optional MODULE MAP
  (per-module statement spans, write-sets, import edges, eager zone).
  Extraction rules come from exp068's grammar spec; the corpus in
  `experiments/068-module-fossils/corpus/` is the fixture source (bun
  is installed at ~/.bun/bin; install elsewhere if needed).
- The **split** consumes the module map as an EXPLICIT INPUT when
  present: bun detected + `--split` ⇒ fossil grouping is the primary
  assignment for app statements. Other bundlers: current path,
  untouched. NO mid-run fallback (the deleted-second-splitter lesson):
  if the map is expected but cannot be built, FAIL LOUDLY.
- A kill switch (`--disable fossil-split`) for rollout safety, wired
  like the existing kill-switch flags.

## Design decisions to settle in task 1 (with data, not taste)

1. **Fossil beats prior-layout inheritance for bun bundles.** Module
   identity comes from the bundle itself each run, so fossil-to-fossil
   layout is stable BY CONSTRUCTION and the prior-tree inheritance
   dependency can drop for bun inputs. First hop after merge pays the
   one-time relayout diff. Verify stability claim with exp068's
   cross-version data before committing to this.
2. **File/folder names key to module identity**: prior-version module
   match (write-set shape + import-edge context — NEVER ordinal) keeps
   the file name; genuinely new modules get named from content. Folder
   structure derives from the init-call DAG + naming; exp068 measured
   the DAG shape as folder-plausible.
3. **The 0.02% without fossils** (eager zone, ~4-8 statements):
   assign by the existing placement tiers — a documented, counted
   residue, not a fallback path.
4. **Vendor boundary unchanged** — vendor factories already have their
   own identity machinery; fossils cover the app wrapper only.

## Cross-version init matching (task 0 — the licensing question)

exp068 deliberately did not compute a placement-churn ceiling because
it needs init MATCHING across versions. Build it first, offline, on
the saved bundles: match inits by write-set shape + import-edge
context; measure match rate and stability on 85→86 and 215→216 (and
the fossil-vs-fossil derived-churn ceiling that falls out). Apply
exp069's stamped rule: **simulate the full delivery funnel on the
churn population** — the ceiling must count derived-churn lines that
fossil layout would actually hold still, not attribute-and-assume.

## Gates (fixed now)

- Statement mass is layout-independent ⇒ **novel/realLn must hold
  byte-exact** vs the pre-relayout reference even across the layout
  change — task 0 verifies this assumption on a prototype tree before
  it becomes the gate.
- **Boot ×4** is the hard safety net; the runnable graph, bun relink,
  and load order must survive relayout.
- Self-hop invariant must stay ≤ current (1 ask / ~2 tree lines) and
  should IMPROVE (placement anchored to module identity).
- e2e fingerprint snapshots and the walk's layout-inheritance
  assumptions are part of the one-time churn: regenerate deliberately,
  never paper over.
- Post-merge: score a NEW cold reference label and re-point the
  standing baseline (a label that says "current main" ages silently);
  steady-state success = the NEXT pair's derived-churn classes
  (importer lines, aliases, export keys), measured fossil-vs-fossil,
  drop materially vs the 69%-of-hidden-churn baseline.

## Cautions pinned before building

- Minified init helpers are found by SHAPE, not name (the pipeline
  renames them).
- Function-only cycles flatten and eager modules leave no fossil —
  bounded small here, but the extractor must COUNT what it cannot
  attribute (rule 8) and the count is part of every report.
- The i36/Pd8 lesson: duplicated modules (two copies of one file) are
  distinct fossils with distinct import edges — identity by caller/
  importer context, never content alone.
- This experiment WILL invalidate layout-dependent comparisons to old
  trees. Only layout-independent columns (novel/realLn, boot,
  self-hop asks) may gate the merge; everything else is re-baselined.

## Task 0 — EXECUTED (worktree, 2026-08-14)

**Matcher** (`init-match.ts`, on `fossil-lib.ts` extraction; signatures =
segment statementHash multisets, disambiguation by import/importer edges
mapped through matches; NO positional tiebreak — unresolved ties stay
unmatched):

| pair | prior/fresh modules | matched | unique-sig | edge-corr | fresh unmatched: twins / low-ov / merged / new |
|---|---|---|---|---|---|
| 85→86 | 3,261 / 3,273 | **3,092 (94.8%/94.5%)** | 2,182 | 908 | 54 / 21 / 7 / 99 |
| 215→216 | 4,820 / 4,850 | **4,699 (97.5%/96.9%)** | 3,863 | 832 | 36 / 16 / 5 / 94 |

**Draw-invariance:** the 85→86 match SET is byte-identical across the
two independent cold runs (exp061-lever-r1 vs r2): 3,092 = 3,092 pairs,
symmetric difference 0. The fossil layer + matcher sit fully upstream
of naming draws.

**Ceiling, full-funnel** (85→86 paired name-only population, 738 pairs
= 1,476 ledger lines): **layout alone holds 4 lines** — re-pointed
aliases co-occur on lines with churned member names (init/export
ordinals), so pure relayout heals almost nothing whole-pair. **Layout +
module-keyed init/export naming holds 360 pairs = 720 lines (49%)** —
token classes: 357 alias-repoints held, 360 members held via matched
declaring module; losses: 307 pairs blocked by plain naming churn
(exp069 territory), 57 by unmatched declaring modules, 11 by unmatched
alias targets. **Design decision 2 (names key to module identity) is
LOAD-BEARING — layout without it is worthless in this population.**

**Rule 8 (what these numbers cannot see):** eager zone 4/4 statements
(85/86) and 8/8 (215/216) unattributable; the ceiling EXCLUDES the
one-sided/relocation mass (055's cross-file masked twins, 1,792
string-anchored lines on r1) — statements that changed FILE are one-
sided in this ledger, so the true fossil benefit is an UNDERCOUNT here;
alias target gate uses any-matched-module over today's ~2.2-module
files (approximation both directions); member lookup: 0 ambiguous, 2
not-found.

**Gate assumption VERIFIED:** `analyze.ts` reads bundles + ledger
hashes only; scrambling every statement's file assignment (seed 42,
1,528 files) leaves the statement columns byte-identical (novel 787,
clean 17,599, churned 1,580) while the layout column moves
259→30,050. novel/realLn are valid merge gates ACROSS a relayout.

**Verdict: implementation is LICENSED**, with the brief's decisions
amended: (1) fossil-primary grouping stands (94.5–96.9% matched,
draw-invariant; unmatched modules get fresh identity); (2) module-keyed
naming is REQUIRED in the same increment, not optional — ceiling
collapses from 720 to 4 lines without it; (3) eager-zone residue stands
(4–8 statements). Combined ceiling ≈ 720 lines on the calm pair vs the
40–60-line floor — 12–18×, the largest licensed lever of the campaign —
PLUS the uncounted one-sided/relocation mass.

## STATUS (2026-08-14): IMPLEMENTED (one increment) + dry-run preview rendered — NOT yet cold-validated

**Shipped on this branch (TDD red-first, 14 new unit tests green):**

- `src/split/fossil-map.ts` — owner: read module fossils from a wrapper
  body (SPEC grammar; helpers by shape; eager zone counted).
- `src/split/fossil-match.ts` — owner: cross-version module matching
  (task-0 tiers ported: unique-signature, edge-corroborated fixpoint,
  high-overlap-unique; no positional rung exists at all).
- `src/split/fossil-assign.ts` — one module = one file; matched modules
  inherit prior paths verbatim; unmatched mint deterministic
  content-stems; folders inferred from the import DAG
  (`inferFossilPlacements`, dominant-importer nesting per the 2026-08-14
  design addendum); eager tail → `src/bootstrap.js`, counted.
- Wiring: `providesModuleFossils` on the unpack-adapter interface (bun:
  true); the decision made ONCE at detection in `unified.ts`
  (adapter capability + `--disable fossil-split` kill switch, registered);
  threaded to `stableSplitFromCode({ fossil })`; ledger records
  `fossilModules` (file + signature + edges) as the next hop's targets.
  Fossil-free bundle under the flag THROWS.

**Dry-run distribution preview (`preview.ts` → `PREVIEW.md`), rendered
from real saved bundles through the SAME `inferFossilPlacements` the
pipeline runs:** 2.1.86 → 3,273 files (94.5% would carry names by module
match, 127 fresh, 54 twins), median 49 lines/file vs current 130;
2.1.216 → 4,850 files (96.9% carry). **The preview's headline finding:
dominant-importer nesting structures ~60% of files into plausible
subtrees, but 1,312 (86) / 2,271 (216) SHARED modules land flat under
`src/` — grouping the shared bucket is the open folder-design question,
and anchor-stem folder names (e.g. `extract-pull-request-number-from-
string/`) need the naming pass Andrew already flagged.**

**Deviations + boundaries, recorded:**

- Eager zone → one `src/bootstrap.js` (bundle-ordered), not "existing
  placement tiers" (amendment 3): those tiers need a prior tree shaped
  like their votes, which a first fossil run lacks. 4–8 statements.
- Init-name stability (the `initializeApp*` mint family) is naming-stage
  work, untouched here — module-keyed FILE identity is what shipped.
- Placement trail/stats report zeros under fossil assignment (the tier
  registry does not run); fossil stats log instead.
- Worktree gate: typecheck/lint/census green; knip and 25 unit failures
  proven ENVIRONMENTAL by stash A/B (identical without the delta; my
  delta = +14 passing tests, and stashing it turns exactly my 3
  integration tests red). Parent gate is authoritative post-merge.

**NOT yet run (main session owns runs):** cold scored run, boot gates,
self-hop under fossil layout. Until those pass, this branch is
implementation + preview, not a validated lever.
