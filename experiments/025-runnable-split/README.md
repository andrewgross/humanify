# Experiment 025 — runnable split: real imports, provable equivalence

**Goal:** finish requirement 3 of the split stage — the emitted tree must
be runnable, with proper imports/exports, not just a folder of parseable
fragments. Two deliverables, in order of certainty:

1. **Concat-equivalence invariant (the guarantee).** Concatenating the
   files in ledger order reproduces the wrapper body byte-for-byte;
   wrapping that back in the original IIFE reproduces the program. This
   is the structural-invariant analog for splitting — provable,
   zero-risk — and makes the tree runnable-as-one immediately.
2. **CommonJS module graph (the navigable form).** Each file exports its
   module-scope declarations and `require`s what it reads; the emitted
   tree has real `require`/`exports` and resolvable, acyclic-where-
   possible imports. Validated per file with `node --check`.

## The corrected structural picture (measured 2026-07-11)

Earlier feasibility numbers (exp023: "946 cross-file-written bindings")
were a name-conflation over-count. Scope-aware analysis
(`probe-scope.ts`, via Babel `constantViolations`) on the real 212-file
tree:

- **29,653 module-scope bindings.** 9,519 are written somewhere.
- **Only 169 (0.57%) are written from a DIFFERENT file** than their
  declaration — the actual hard cases.
- Those 169 are overwhelmingly **within one folder** (split pieces of a
  single original module: `arrayBuilder/eventLogger.js` ↔
  `arrayBuilder/errorProcessor.js`). So cross-file mutation is a
  local, containable phenomenon, not a pervasive one.

Forward references (a binding used before its declaration's file loads)
are common — one hoisted scope — but CommonJS tolerates them as long as
the reference executes after all modules load (function bodies), which
is the normal case; only module-load-time top-level reads are at risk.

## Design

### Concat-equivalence (build + prove first)

The ledger's `order` array already records every statement's file in
statement order. Emission slices exact bytes. So `concat(files in
`order`, joined by "\n") === wrapper body` must hold by construction —
assert it as a test over the real tree and as a unit invariant. Ship a
`reconstruct` that rebuilds the runnable single file from the tree +
ledger; a smoke test parses it and checks structural identity to the
original.

### CommonJS module graph

Per file, from the AST + ledger:

- **Exports:** its module-scope declared bindings that are referenced
  from any OTHER file → `module.exports = { a, b, … }` (or
  `exports.a = a`). Bindings used only within the file stay private.
- **Imports:** names it references that are declared in another file →
  `const { x } = require("./relative/path.js")`, grouped per source
  file, relative paths computed (reuse the legacy emitter's
  `computeRelativeImportPath` if it generalizes — CLAUDE.md unify rule).
- **The 169 cross-file writes:** a required binding is read-only in the
  importing file. Emit those specific writes as namespace-qualified
  assignments on the imported module object
  (`mod.foo = …` where `const mod = require("./decl.js")`), which
  CommonJS DOES propagate — a mechanical, semantics-preserving
  transform (module extraction, not LLM rewriting). Where even that is
  unsafe (write to a `const`, or a destructured import), fall back to
  keeping the writer's statement in the binding's file (a placement
  nudge recorded in the ledger, so it is stable next release).

### Validation (precision over recall)

- Concat-equivalence assert must pass or the whole split is rejected.
- Every emitted file `node --check`-clean (parse + early-error pass).
- Import graph resolves (every `require` target exists); cycles are
  allowed (CommonJS handles them) but reported.
- Honest scope: syntactic + link-level runnability is the bar. Full
  execution of the Claude Code binary as split modules (circular-dep
  load-order hazards on a bundle this dense) is measured and reported,
  not promised — precision over recall.

## Metrics + success

- Concat-equivalence: byte-identical reconstruction (pass/fail).
- `node --check`: 212/212 files clean.
- Imports resolve: 100% of `require` targets exist.
- Cross-file writes handled: all 169 either namespace-qualified or
  colocated; zero silent drops.
- Stability unchanged: the export/import blocks are deterministic from
  the ledger, so re-emission across releases adds no churn beyond the
  genuine binding-set delta (measure it).

## Runbook

```bash
npx tsx experiments/025-runnable-split/probe-scope.ts \
  /tmp/e022/120F.js /tmp/e024/120-llm/_split-ledger.json
# emitter harness (to build): emit CJS tree + validate
```

## Code anchors (verified 2026-07-11 on main 8c4edd4)

- Split core: `src/split/stable-split.ts` — `emitFiles` (byte slicing),
  `buildLedger` (`order` array), `StableSplitLedger`.
- Legacy emitter to reuse: `src/split/emitter.ts` —
  `computeRelativeImportPath`, `generateImports`, `generateExports`,
  `generateBarrelIndex`; `collectReferencedNames`,
  `extractDeclaredNames`.
- Scope/writes: Babel `Binding.constantViolations`,
  `Binding.referencePaths`.
- Validation harness precedent: `experiments/validate-split.ts`.

## Out of scope

- Re-clustering / moving code for cleanliness (placement is exp023's;
  only the 169-write colocation fallback nudges, and it is ledgered).
- Renaming (bindings or files) — exp014–024.
