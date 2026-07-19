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

## Part 2 — SOLVED: release the naming-era AST before the re-parse passes

Linchpin (bench-live-postnaming.mts --free-ast): freeing the main AST before
the post-naming passes drops validateOutput 45s→12s and reconcile 53s→15s
(fresh-heap levels). Freeing only the graph did nothing — the AST is the bulk.

Fix (plugin.ts + processor.ts): right after `generate` produces output.code,
release every holder of the bundle AST — the `ast`/`graph`/`allFunctions`
locals AND `processor.ast` (new `RenameProcessor.releaseAst()`; reports and
RenameDecision hold no nodes, verified). The post-naming passes work in
output-code space; `resolveFinalOutput`'s finalAst comes from reconcile/sweep
or a re-parse of the (unchanged) shipping code. `--emit-rename-ledger` keeps
the original AST (its base stage indexes into it). Output is byte-identical —
the release is memory-only, after the code string is generated.

VALIDATION — the full 2.1.207→208 run (hung 5× before) COMPLETED, exit 0,
~18min: `Stable split: 1487 files / 102 folders [runnable CJS + Bun re-link]
— inherited 31341/34023 (19720 via hashes)`; boots under Bun
(`2.1.208 (Claude Code)`). 1346 unit + 33 fingerprint green.

NOTE (separate, pre-existing): the 208 tree does not boot under plain `node`
(require-graph stack overflow) — but the ARCHIVED 208 AND 196 trees (old code,
no branch changes) fail node-boot too, while all boot under Bun (the target
runtime). Not a regression from this work; a cross-version node-boot item to
track independently.

## Scripts

- `stress.mts` — drop-per-cycle loop; measures GC sawtooth, NOT a clean red/
  green (both arms heap-bound). Kept as a caution.
- `stress-live-main.mts` — holds one AST live + re-parses; RAW arm 9s→41s→75s→
  hang, FUNNEL arm 9s→39s→63s→climbing. Both degrade = the harness is
  heap-pressure-bound, not tombstone-bound; do not read as a fix test.
- `bench-live-postnaming.mts` — THE faithful localizer: real passes with the
  main AST held live. `--free-graph` to test shedding the graph.
