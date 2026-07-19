# 031 — Per-parse cache-era funnel (ephemeron hang, part 1 of 2)

## What this landed

A single parse funnel in `src/babel-utils.ts` (`parseSourceAst`, and
`parseFileAst`/`transformWithPlugins` routed through it) that starts a fresh
cache era whenever a full bundle (`code.length >= BIG_SOURCE_BYTES` = 5MB) is
parsed: swaps the three registered node-keyed WeakMaps (node-caches.ts) AND
Babel's internal path/scope cache. One deliberate opt-out — the prior-version
parse (`preserveAstCaches: true`), which is born while the new AST's warm
entries are still needed by matching and the hermetic invariant. Deleted the
lone manual `resetAnalysisNodeCaches()` at unified.ts. Call sites A,C,D,E,H,I
plus the pre-rename babel transform (site J) now funnel; small per-file parses
(split slices, using-desugar, ~1500 relink files) stay below the gate.

`npm run check` EXIT=0; 1346 unit + 33 fingerprint green. This is a correct,
tested improvement that removes a real hazard — but see the verdict.

## VERDICT: necessary, NOT sufficient. NOT merged. Backfill NOT started.

The full 207→208 production run (the gate) **still hung** post-naming with the
same `Rehash`/`WeakCollection` signature. The funnel removed the tombstones;
a SECOND, independent cause remains.

### Localization (bench-live-postnaming.mts — the harness the earlier bench missed)

Holding the MAIN 30MB AST + graph LIVE (as the pipeline does through
validate/reconcile), then running the REAL passes WITH the funnel active:

| pass                       | fresh heap (bench-postnaming) | main AST held live |
| -------------------------- | ----------------------------- | ------------------ |
| captureSemanticBaseline    | ~8s                           | 6s                 |
| validateOutput (re-parse)  | ~8s                           | **45s**            |
| runPriorDiffReconciliation | ~14s                          | **53s**            |

They COMPLETE (~1.7min total) but 5× slower — even though the funnel reset
their caches. RSS climbs 4→8GB (NOT exhaustion; 14GB cap). This is
live-main-AST GC pressure (project_walk_slowdown class): each pass allocates a
fresh 30MB AST while ~4GB of main AST is live, so every major GC re-traces
that live set. The full pipeline holds MORE live state (graph + processor +
fingerprint index) and tips from "5× slow" into the nondeterministic spiral.

### The cheap fix does NOT work

`--free-graph` (drop the graph before the post-naming passes — it is dead
after the rename pass): validate 44s, reconcile 64s — unchanged. The graph is
not the bulk; the **main AST** is, and generate/ledger/census still need it.
So the fix is not a null-a-reference change.

## Part 2 (the remaining work, NOT done)

Reduce the live set during the post-naming re-parses. Options, in rough order
of risk:

1. Free processor + fingerprint-index refs after naming (like the graph —
   likely dead; verify). Probably insufficient alone (AST is the bulk).
2. Restructure so the main AST is released BEFORE validate/reconcile — but
   generate (needs AST), the pre-generate structural invariant, and the
   post-reconcile ledger/census all need it. Requires reordering (run
   ledger/census earlier, or move validate/reconcile after AST release) with
   pure-rename-invariant correctness risk.
3. Rethink whether validate/reconcile must allocate a second full AST at all.
   Validate any candidate with bench-live-postnaming.mts (2min, no LLM) BEFORE a
   full run.

## Scripts

- `stress.mts` — drop-per-cycle loop; measures GC sawtooth, NOT a clean red/
  green (both arms heap-bound). Kept as a caution.
- `stress-live-main.mts` — holds one AST live + re-parses; RAW arm 9s→41s→75s→
  hang, FUNNEL arm 9s→39s→63s→climbing. Both degrade = the harness is
  heap-pressure-bound, not tombstone-bound; do not read as a fix test.
- `bench-live-postnaming.mts` — THE faithful localizer: real passes with the
  main AST held live. `--free-graph` to test shedding the graph.
