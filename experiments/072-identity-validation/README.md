# 072 — is "identical" ever WRONG? (ground truth + bundler generality)

> **This is a BRIEF — a hypothesis, including its cautions.**
>
> Andrew, 2026-08-15: every persistence number so far
> (exp070/071: 80% of files provably identical per release, 66–87%
> range over 123 hops) was measured on Claude Code, **where we do not
> know the true source files**. Those numbers show our fingerprints are
> SELF-CONSISTENT. They do not show they are CORRECT. Before any
> mechanism carries names on the strength of "identical", the verdict
> itself must be validated against a known answer — and the natural
> place to get one is the generality test, because a bundle we build
> ourselves comes with its source.

## The two questions, and why they are one experiment

1. **Is the identity verdict correct?** Build version A and version B
   from source we control, with a KNOWN edit list. For every module,
   compare our verdict (identical / changed / ambiguous) against truth.
2. **Does any of this generalize past Bun?** Build the same sources
   with esbuild (whose `__esm`/`__commonJS` helpers Bun's derive from —
   the reader may work unmodified) and rollup (the honest hard case,
   which may erase boundaries entirely).

## Error classes — they are NOT symmetric

- **False identical** (we say unchanged; truth says different file or
  changed content) — DANGEROUS: a name-carry would pin a stale or
  foreign name. Target: zero. Any occurrence is a design constraint,
  not a tuning knob.
- **False changed** (we say changed; truth says untouched) — SAFE:
  coverage lost, nothing broken.
- **Boundary error** (module ≠ one source file): over-merge, split, or
  miss. Bounds everything downstream.

## Mutations the corpus must exercise

Each is a known-truth case, chosen because it probes a specific claim
the fingerprint makes:

| mutation | truth | what it tests |
| --- | --- | --- |
| rename a local/param | file UNCHANGED in behaviour, names differ | fingerprints mask names — must read identical |
| change a string/number literal | CHANGED | literals ARE hashed (unlike `structuralHash`) |
| add/remove a statement | CHANGED | shape sensitivity |
| reorder independent statements | CHANGED (set-of-hashes may miss it) | **suspected blind spot** — module sig is a SORTED set |
| duplicate a file verbatim | two files, indistinguishable | the ~12.5% twin class, with truth attached |
| move a file (path only) | UNCHANGED content | path-independence |
| upgrade a dependency version | dep files CHANGED, app files UNCHANGED | the real-world mixed case |

## Cautions pinned before measuring

- A synthetic corpus can be too clean: real minified code has helper
  hoisting, cross-module inlining and shared constants. Run the same
  battery on a REAL package's published versions (registry is
  reachable) before believing a zero.
- Bun only emits per-module initializers when modules are LAZY
  (verified 2026-08-14: a plain ESM import is inlined; one dynamic
  import produced exactly one initializer). The corpus must force the
  same laziness the real target has, or it validates a shape we never
  meet in production.
- Rollup may leave no boundaries at all. That is a RESULT (the reader
  cannot support it), not a failure to fix by guessing.

## Success criterion (fixed now)

A per-mutation table of verdict vs truth on ≥2 bundlers, with the
false-identical count stated explicitly. Zero false-identicals licenses
name-carrying by identity; any non-zero count names the exact mutation
that breaks it and bounds where carrying is safe.
