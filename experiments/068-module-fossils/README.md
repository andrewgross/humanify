# 068 — module fossils: the bundle still contains the original file layout

> **This is a BRIEF — a hypothesis, including its cautions.**
>
> Andrew's insight (2026-08-14): the `initializeApp*` family are the
> bundler's per-module lazy initializers — ONE PER ORIGINAL SOURCE
> FILE. Each init's body initializes exactly its module's top-level
> bindings, and init-calls-init IS the original import graph, minified.
> The split currently reconstructs module boundaries STATISTICALLY
> (clustering / name votes / hashes) while the bundle contains them
> LITERALLY. Reading the fossils could replicate the original
> project's file distribution, anchor placement to module identity
> (killing a churn class at its root), name files from real module
> content, and subsume much of 067's export-surface problem.

## Ground rules

- **Bun-specific by design** — this belongs in the unpack/detection
  plugin architecture (the two real registries), as upfront detection
  feeding the split, NEVER as a mid-run fallback (the deleted second
  splitter's lesson: fail loud, don't re-split cruder).
- **Expect mess.** Minifiers inline/merge some inits; hoisted function
  declarations live OUTSIDE the closures with only export wiring
  inside; some modules may have no init at all (constants-only,
  fully-hoisted). Attribution needs the init's write-set, not just its
  body.

## Tasks

0. **Corpus with an answer key (Andrew's method):** take source we
   CONTROL (a small multi-file project, then a mid-size real one),
   bundle it with Bun ourselves across its option matrix (minify
   on/off, splitting, target), and learn the exact fossil grammar:
   init shape, write-set → module mapping, hoisted-declaration
   attribution, what survives minification. Deliverable: a written
   spec of the reconstruction rules with per-rule confidence.
1. **Census on the real bundles:** how many inits per Claude Code
   version; % of app statements attributable via write-sets; init↔
   statement grouping stability across 85→86 (should beat clustering);
   init-call DAG shape vs plausible folder structure.
2. **Design the integration:** detection-time module map → split
   consumes it as the primary grouping (prior-layout inheritance
   still wins for existing files? or migrate wholesale — decide with
   data); file/folder naming from module content; vendor boundary
   cross-check.

## Cautions pinned before measuring

- Rule 8: enumerate what the fossil CANNOT see (hoisted-only modules,
  merged inits, vendor CJS factories — already handled elsewhere)
  before believing a coverage number.
- Layout migration is a one-time churn (not a cost) but the NEW
  anchor must be stable ACROSS VERSIONS — measure grouping stability
  (task 1) before any integration.
- The walk/e2e machinery assumes the current layout inheritance;
  integration is a later, separately-gated experiment.

## Success criterion (fixed now)

Task 0 produces the fossil-grammar spec validated against known
source; task 1 produces coverage + stability numbers on two real
pairs, two-run stable. NO pipeline integration in this experiment —
the map licenses the next one.
