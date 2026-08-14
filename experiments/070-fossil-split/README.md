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
