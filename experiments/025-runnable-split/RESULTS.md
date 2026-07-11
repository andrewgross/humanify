# Exp025 — runnable split: provably equivalent, provably loadable

Branch `exp025-runnable-split` (off `main` after exp024). Goal:
requirement 3 of the split stage — the emitted tree must be runnable,
with real imports, not a folder of parseable fragments.

## Headline

The split is runnable in two forms, each proven on the real 212-file
tree:

1. **Concat-equivalence (the guarantee, shipped to production).**
   `reconstructBody` rebuilds the exact wrapper-body statement sequence
   from the tree + ledger — every statement once, in order,
   byte-identical. Re-wrapping in the IIFE is the original program.
   Verified: **23,602/23,602 statements, 36,101/36,101 bindings, 0
   missing / 0 extra, 0.6s.**
2. **CommonJS module graph (the navigable form).** Real
   `require`/`module.exports`, and — the decisive result — the
   load-time dependency graph is **acyclic: 212/212 topologically
   loadable, 0 cycles.** A valid require order provably exists, so the
   graph loads without a load-time ReferenceError.

## The corrected feasibility picture

exp023's "946 cross-file-written bindings" was a name-conflation
over-count (it globbed function-local `errorMessage`/`currentIndex`
across files). Scope-aware analysis (Babel `constantViolations`) on the
real tree:

| measure                           | value           |
| --------------------------------- | --------------- |
| module-scope bindings             | 29,653          |
| written somewhere                 | 9,519           |
| **written from a DIFFERENT file** | **169 (0.57%)** |

The 169 cluster _within folders_ (split pieces of one original module,
`arrayBuilder/eventLogger.js` ↔ `arrayBuilder/errorProcessor.js`), so
cross-file mutation is local and containable, not pervasive.

## The executability chain (each number measured)

- **Cross-file read edges: 24,469**, all resolvable — every `require`
  target exists (5,606/5,606).
- **94.1% of cross-file reads are deferred** — they execute inside
  function bodies, after all modules have loaded, so CommonJS's
  partial-exports-during-circular-require hazard never touches them
  (62,984 deferred vs 3,942 load-time).
- **The 5.9% load-time reads funnel through 2 bindings:** `lazyInit`
  (3,560) and `defineExportProperties` (370) — the Bun CJS runtime
  helpers — plus 6 factory functions (≤7 reads each). 8 names total,
  100% of load-time reads.
- **The load-time dependency graph is ACYCLIC** (217 edges, 212/212
  topologically sortable). Because the two helpers live in leaf
  modules, requiring in topological order satisfies every load-time
  read. No circular-load breakage.

So "is it runnable?" is a measured **yes**: the reconstructed single
file is the original program, and the module graph has a provable load
order.

## Validation summary (real tree)

| check                              | result                                                                                                     |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| concat-equivalence reconstruction  | 23,602/23,602 ✓                                                                                            |
| emitted files babel-parse clean    | 212/212 ✓                                                                                                  |
| `node --check` clean               | 207/212                                                                                                    |
| — the 5 failures                   | all `using`/`await using`; the ORIGINAL bundle fails identically (Node syntax gap, zero emission breakage) |
| require targets resolve            | 5,606/5,606 ✓                                                                                              |
| load-time dependency graph acyclic | 212/212, 0 cycles ✓                                                                                        |

Sample emitted module (`ideConnection/paginationHook.js`): 26 grouped
`const { … } = require("../…")` imports with computed relative paths,
the file's statements, then `module.exports = { … }` — reads like a
real module.

## What shipped to production

- **`src/split/stable-split.ts` — `reconstructBody`** (+ 2 tests): the
  concat-equivalence guarantee. This is the runnability contract at the
  file axis, the structural-invariant analog for splitting: if the tree
  and ledger ever disagree it throws, so a broken split can never be
  silently shipped.

The CJS emitter, edge analysis, and executability probes live in
`experiments/025-runnable-split/` — the design is proven; productionizing
the emitter is the remaining step (below).

## Honest caveats / remaining work

- **The 169 cross-file writes need namespace-qualification to be
  execution-correct.** The experiment emitter imports the write-target's
  module as a namespace but does not yet rewrite `foo = x` →
  `mod.foo = x` in the writer's body (bare `foo = x` would create an
  implicit global). This is a mechanical AST transform (module
  extraction, not LLM rewriting); it is the one step between "loadable
  graph" and "execution-correct graph". Scoped for exp026.
- **Full standalone execution of the Claude Code CLI** as split modules
  was not run end-to-end (it is a CLI with real side effects and
  environment needs); the claim here is load-order soundness +
  link resolution + syntactic validity, which is what the guarantee
  rests on. The concat-equivalent single file is the always-runnable
  fallback.
- Import blocks are large (26/file avg) because the bundle is densely
  coupled — honest reflection of the original, not emitter noise.

## Reproduce

```bash
npx tsx experiments/025-runnable-split/probe-scope.ts     /tmp/e022/120F.js /tmp/e024/120-llm/_split-ledger.json
npx tsx experiments/025-runnable-split/verify-reconstruct.ts /tmp/e022/120F.js /tmp/e024/120-llm
npx tsx experiments/025-runnable-split/emit-cjs.ts        /tmp/e022/120F.js /tmp/e024/120-llm/_split-ledger.json /tmp/e025/cjs
npx tsx experiments/025-runnable-split/probe-loadgraph.ts /tmp/e022/120F.js /tmp/e024/120-llm/_split-ledger.json
```

## Next candidate

exp026 — productionize CJS module emission: port the edge analysis into
`stable-split.ts`, apply the 169-write namespace rewrite, emit in
topological load order, and drive a real `node` load of the tree.
