# 067 — shrink the export/alias surface we emit

> **This is a BRIEF — a hypothesis, including its cautions.**
>
> Andrew, reading the exp066-era diffs (2026-08-14): a large share of
> tree bulk and some churn is our own plumbing — per-name export
> assignments, namespace alias declarations, re-export hops — repeated
> for every module instance (React shims, the `initializeApp*` family).
> The export surface is OUR construction: the runnable tree's shape is
> a layout choice, so consolidation here is behavior-preserving by
> design, gated by boot.

## Tasks

0. **Census (measure first):** per tree — export assignments, alias
   declarations, re-export chain hops (A re-exports B re-exports C),
   and the diff lines riding on each across the four pairs. How much
   is bulk (stable, just noisy to read) vs churn (unstable across
   hops)?
1. **Collapse re-export chains to direct references** where the
   intermediate adds nothing (target-identity-aware — exp062's lesson:
   same-shaped is NOT same-thing; never collapse by hash).
2. **Prune the per-file export block** to names actually imported
   somewhere (the closed runnable graph is known at emit time; dead
   exports are pure bulk). Boot gates are the safety net.

## Cautions pinned before measuring

- Export keys are load-bearing strings (top-level renames never carry
  — 238/238 drifted); consolidation must not rename keys, only remove
  indirection.
- The `initializeApp*` bodies are side-effectful load-time code —
  their CALL ORDER is the original import order; pruning must not
  reorder or elide calls (boot + the load-order machinery guard this).
- Depends on exp068's findings if module-boundary reconstruction
  changes what a "file" is — sequence 067 AFTER 068's census.

## Success criterion (fixed now)

Census first; any collapse ships only with boot ×4 OK,
`novel`/`realLn` byte-exact, and a measured tree-size / churn
reduction attributable to the collapsed classes. One-time relayout
churn is not a cost (phase rule, 2026-08-13).
