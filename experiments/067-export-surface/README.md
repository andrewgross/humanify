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

## STATUS (2026-08-14): EXECUTED — census complete; BOTH consolidation candidates are VOID; no code shipped

The brief's premise ("a large share of tree bulk and some churn is our
own plumbing") was half right — the bulk is real, the consolidation
targets are not:

**Census (`census.ts`, exp066-r1 trees):** the export surface is
30,052 of 466,233 lines on 2.1.86 (6.4%): 19,015 alias-decl lines +
11,037 export-def lines. 2.1.85-rebased is the same shape (30,261 of
463,016).

**Candidate 1 — dead-export pruning: VOID BY CONSTRUCTION.** The
emitter only exports a binding with cross-file reads or writes
(`planBinding`: `readCrosses || writable` → `plan.cross`), so dead
exports cannot exist. Measured 0 of 11,037 on both trees; the probe
was validated (a sampled export IS accessed by its importer; the
liveness scan finds it). There is nothing to prune.

**Candidate 2 — re-export chain collapse: POPULATION ZERO in app
code.** No `get: () => alias.member` forwarding exports exist in any
app file; the only forwarders are vendor stubs, whose churn exp063
already sized at 10–12 lines (skip). There are no chains to collapse.

**Churn exposure by class (system diff, all four pairs):**

| pair | total diff ln | alias-decl ln | same-target name-only pairs | export-def ln | dead-export ceiling |
|---|---:|---:|---:|---:|---:|
| 85→86 | 39,839 | 3,123 | 14 (=28 ln) | 1,205 | 0 |
| 118→119 | 36,261 | 1,186 | 3 (=6 ln) | 937 | 0 |
| 197→198 | 41,336 | 1,782 | 24 (=48 ln) | 991 | 0 |
| 215→216 | 24,769 | 835 | 30 (=60 ln) | 626 | 0 |

The strict ceiling for the one live class (same-target alias renames)
is 6–60 lines per pair — at or under the 40–60-line repeat spread on
every pair. Sized skip, consistent with exp063's ordinal-carry
verdict. The REST of the alias/export churn (~2,000–4,000 ln/pair) is
aliases whose TARGET changed or one-sided file adds/removes — a
downstream symptom of placement and layout, which is exp068-integration
territory, not consolidatable plumbing.

**What did not survive:** both of the brief's tasks. What stands: the
bulk number (6.4% of the tree is plumbing, all of it live) — if that
bulk is to shrink, the lever is the fossil-guided relayout (fewer,
truer files change what needs exporting), not surface consolidation.

Instrument: `census.ts` (census + churn modes, dead-export liveness
with conservative escape detection).
