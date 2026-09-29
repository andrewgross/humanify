> **Recovered 2026-09-29 from branch `072-...` before its deletion** (the
> experiment lived on a worktree branch; conclusions are the record — code preserved as-is,
> superseded by the Rust cutover where imports changed). STATUS header below is from its time; see
> /work/post-cutover-notes.md for the 2026-09-29 recovery.

# 072 — is "identical" ever WRONG? (ground truth + bundler generality)

> **STATUS (2026-08-15): EXECUTED — ZERO false identicals, on both a
> corpus we control and a real package pair. The verdict is licensable,
> with two named boundaries.**
>
> **The headline: 0 modules where we said "identical" and the emitted
> code really differed** — synthetic (150 files, known edit list) and
> real (date-fns 3.3.1 → 3.6.0, 1,066 → 1,082 modules, truth from
> published bytes), on bun and esbuild alike.
>
> Every apparent mismatch dissolved under an emit check that the harness
> now performs automatically on EVERY identical verdict:
>
> - **9 literal changes read identical** because the literal was dead
>   code the bundler eliminated — emitted text byte-identical.
> - **5 import re-points read identical** — emitted text identical or
>   differing only in a generated alias index (`import_ms5` →
>   `import_ms3`), which the fingerprint masks by design.
> - **4 real-package files read identical** — the only source change was
>   a JSDoc line, and comments do not survive bundling.
>
> That reframes the claim precisely: **the fingerprint answers "is the
> EMITTED module the same", not "is the SOURCE FILE the same"** — and
> the emitted module is the only artifact we ever see, so for carrying
> names it is the correct question.
>
> **The predicted blind spot did NOT appear.** Statement REORDER was
> expected to read identical (a module signature is a sorted set) and
> read CHANGED 14/14: within-module order lives inside the initializer's
> single statement hash, so only wrapper-level reordering could hide,
> and there is none. Sorted-set exposure is narrower than feared.
>
> **Boundaries, both real:**
>
> 1. **Ambiguous twins scale with the codebase's shape** — 12.5% on
>    Claude Code, **36% (372/1,039 unchanged files) on date-fns**, whose
>    hundreds of tiny wrapper modules are structurally identical. Any
>    carry must leave twins alone, and the cost of doing so is
>    project-dependent, not a fixed 12%.
> 2. **False CHANGED is common and harmless** — 372 unchanged real files
>    read ambiguous rather than identical. Coverage lost, nothing broken.
>
> **Generality (the reader is bun-shaped, and that is now measured):**
>
> | bundler             | modules found | note                                                                                             |
> | ------------------- | ------------- | ------------------------------------------------------------------------------------------------ |
> | bun, unminified     | 154/154       |                                                                                                  |
> | bun, minified       | 154/154       | minification does not change fossil structure                                                    |
> | esbuild, MINIFIED   | 154/154       | works unmodified                                                                                 |
> | esbuild, unminified | **0**         | wraps as `__esm({ "src/x.js"(){…} })` — object + keyed method; the reader wants bun's arrow form |
> | rollup              | **0**         | boundaries erased entirely; no wrappers, no path comments — genuinely unsupported                |
>
> Two consequences worth acting on: esbuild support is a small reader
> variant, not a project; and esbuild's unminified form embeds **the
> source path as the object key**, so any esbuild build that ships
> unminified hands over the file layout for free.

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

| mutation                       | truth                                     | what it tests                                         |
| ------------------------------ | ----------------------------------------- | ----------------------------------------------------- |
| rename a local/param           | file UNCHANGED in behaviour, names differ | fingerprints mask names — must read identical         |
| change a string/number literal | CHANGED                                   | literals ARE hashed (unlike `structuralHash`)         |
| add/remove a statement         | CHANGED                                   | shape sensitivity                                     |
| reorder independent statements | CHANGED (set-of-hashes may miss it)       | **suspected blind spot** — module sig is a SORTED set |
| duplicate a file verbatim      | two files, indistinguishable              | the ~12.5% twin class, with truth attached            |
| move a file (path only)        | UNCHANGED content                         | path-independence                                     |
| upgrade a dependency version   | dep files CHANGED, app files UNCHANGED    | the real-world mixed case                             |

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
