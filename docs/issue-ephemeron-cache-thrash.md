# Issue: ephemeron WeakMap tombstone thrash (O(n²) hang/slowdown at scale)

Status: **structurally fixed** on this branch (feat/per-ast-analysis-cache).
The three module-level node-keyed WeakMaps are gone, replaced by per-AST
caches (`src/analysis/analysis-cache.ts`): one plain-Map cache per AST root,
collected wholesale with its tree. There is no shared table for tombstones to
densify and no reset API left to call — the reset choreography below
(`resetAnalysisNodeCaches`, `resetNodeCachesAfterPriorMatch`, the funnel's
analysis-cache half, `preserveAstCaches`'s analysis semantics) is deleted.
Babel's own module-level path/scope cache is the one node-keyed table we
cannot restructure; the existing big-parse/boundary clears remain for it, and
hashing is now era-proof against those clears (slot placeholders key by
declaration node, so a walk mixing pre- and post-clear scope resolutions
still unifies — previously a latent correctness hazard).

Re-measured on the real 2.1.207/208 bundles (experiments/032/RESULTS.md,
"Per-AST re-measurement"): the prior-build window drops 214 s → 156 s — the
analysis-table share of the dense-table penalty, gone structurally. The
remaining ~3-4× inflation over a light-heap build (37-46 s) is **live-heap GC
tracing**, not table pathology: clearing Babel's table barely moves it, and a
post-drop build stays ~170 s even against an empty table, because every GC
during the build re-traces the several-GB still-live other AST + graph. That
term is bounded and deterministic (a tax, not a hang); the structural lever
for it is building the prior FingerprintIndex in a SUBPROCESS (everything the
matcher consumes from the prior side — fingerprints, statement hashes,
placeholder name maps, close-match context — is serializable), tracked as
follow-up. The sections below are the original analysis, kept for history.

Last updated 2026-07-20 (during RUN 4 of the claude-code version walk; branch
work done in a worktree, main untouched).

## Symptom

A hop's worker process pins **one core at 100% CPU**, the log **freezes
mid-phase**, RSS goes **flat** (NOT climbing — it is _not_ OOM), the LLM
server's GPUs sit **idle** (so it is not waiting on the model), and
`sample <pid>` shows V8's \*\*`ObjectHashTable::Rehash` + `Runtime_WeakCollectionSet`

- `GetIdentityHash`\*\* dominating. It is quadratic, not an infinite loop: a
  21 MB hop grinds through in ~50 min; a 30 MB hop would take hours (an early
  2.1.207→208 test sat 20–25 min in a post-naming pass before it was killed).
  Nondeterministic — most hops are fine (~8 min), a fraction spike.

## Root cause

The analysis layer memoizes hashing in **three module-level, AST-node-keyed
WeakMaps**, created once at import and reused for the whole process:

- `bindingByIdentifierNode` — src/analysis/structural-hash.ts (one entry per
  identifier occurrence → millions per bundle)
- `stmtHashByNode` — src/analysis/fingerprint-index.ts
- `shingleSetCache` — src/analysis/function-fingerprint.ts

Babel keeps its own node-keyed path/scope cache too (cleared via
`clearBabelTraverseCache()` in src/babel-utils.ts).

The pipeline parses the 17–32 MB bundle **many times per hop** (input parse,
prior-version parse, output re-parse for validate/reconcile/sweep, split
re-parse, per-file emit, Bun re-link). Each time an AST is parsed then dropped,
its millions of node keys become **tombstones** in the persistent WeakMap. A
WeakMap's backing table is sized to its historical peak, so after a big AST
dies the table is huge-capacity but mostly dead slots. When the _next_ phase
bulk-inserts millions of fresh keys, V8 collides with tombstones and **re-hashes
the entire backing table on nearly every insert → O(n²)**. That rehashing is
pure single-threaded CPU, which is the exact signature above.

It is _not_ OOM and more RAM does not help — a bigger heap lets the tombstone
table grow larger, making each rehash worse. It is a "take the trash out
between phases" problem, not a "buy a bigger truck" problem. See
docs/../ (memory: project_ephemeron_cache_fix) for the running history.

## Why it surfaced now

Two things had to line up, and only did in mid-2026: (1) bundles grew to ~30 MB
beautified (v186+); (2) the diff-reconcile + minted-sweep passes — each an
extra full-bundle re-parse — were defaulted **on** (commit d47013c). A prior
full walk predated (2) and never hit it; a later run stopped below the size
threshold. "Current code + 30 MB" first ran on the 2.1.207→208 hop.

## Attempted fixes (all landed on main)

The strategy so far: **reset the caches at each boundary where an AST dies**, so
tombstones cannot accumulate before the next heavy phase.

1. **Split-boundary reset** (4dbfcbc). `resetAnalysisNodeCaches()` at the top of
   `tryStableSplit`. Fixed the _split_ hang. First discovered site.
2. **Per-parse cache funnel** (f0ceeab, exp031 part 1). `parseSourceAst()` in
   babel-utils: any parse ≥ 5 MB resets the 3 caches + Babel's cache first,
   UNLESS `opts.preserveAstCaches`. Routes every full-bundle parse through one
   choke point so new sites are covered automatically. Necessary but **not
   sufficient** on its own — the 207→208 gate still hung.
3. **Release the naming-era AST before the post-naming re-parses** (81397bd,
   exp031 part 2). After `generate()` the plugin nulls its `ast`/`graph`/
   `allFunctions` locals + `processor.releaseAst()`, so validate/reconcile/sweep
   re-parse on a light heap (their live-AST GC tax dropped validate 45→12 s,
   reconcile 53→15 s). This is what made 207→208 **complete** (18 min, boots
   under Bun, real API round trip).
4. **Prior-match → naming reset** (ade8eae, exp032 part 3).
   `resetNodeCachesAfterPriorMatch(options.priorVersionCode)` after
   prior-version matching drops the prior AST, before `runRenamePass` — the
   naming pass was inheriting the prior AST's tombstones. Killed the
   _catastrophic_ naming-phase thrash (prod hop 2.1.107 sat ~40 min here).

Each patch is validated by `npm run check` (1346 unit + 33 fingerprint) plus,
for 3 and 4, mechanism benches in experiments/031 and experiments/032 on real
30 MB bundles, and the pure-rename structural invariant guarantees output is
byte-identical (the resets are memory-only; the caches are pure deterministic
memoization, so a reset only forces recompute-on-demand).

## What's fixed vs what remains

Fixed: the _hard hangs_ — split, post-naming, and the catastrophic naming
thrash. Before the patches, hops hit 49–57 min stalls; after, nothing exceeds
~40 min and 207→208 completes.

**Residual (open):** a nondeterministic ~28–40 min slowdown on ~1/3 of the
larger hops, with the time spent in **non-naming, local-CPU phases**
(prior-match, graph builds, split), not LLM naming. Evidence: hop 2.1.161
(26 MB) took 37 min with only ~5 min of LLM naming; and it ran _slower_ than
2.1.207→208 (32 MB, 18 min) — a smaller version beating a bigger one means it
hit thrash the bigger one's GC timing happened to dodge.

## Why the reset approach has a ceiling

The **prior-version matching phase cannot be reset.** Matching deliberately runs
the same hash functions over BOTH the new and prior ASTs at once (to compare
them), so it needs both ASTs' cache entries live simultaneously — that is why
the prior-bundle parse uses `preserveAstCaches: true` and the funnel skips it.
Consequently the prior graph build inserts millions of prior-AST keys into a
table already dense with the new graph's keys. Measured (experiments/032):
`buildUnifiedGraph` into a **fresh** table = 49 s, into the **new-filled**
table = **214 s (4.4×)**. No boundary reset can remove this — the density is
required by the algorithm at that moment. This is the structural floor of the
reset strategy and the main source of the residual slow hops.

## The real fix (not yet done): per-AST cache scoping

Replace the module-level singleton WeakMaps with caches whose lifetime is
**one AST**, so a dropped AST's entries die _wholesale_ (as one collectable
object) instead of becoming tombstones in a shared table. Two shapes:

- **Two-level `WeakMap<ProgramRoot, WeakMap<Node, X>>`** keyed by AST root. When
  a root dies, its inner map (millions of entries) is collected as a unit — no
  tombstones, no manual resets anywhere, and it even fixes the prior-match case
  (new and prior get separate inner maps, so neither densifies the other).
  Cost: the hot lookup (per identifier, millions of calls) must resolve
  node → root first; Babel's own path cache is shaped this way and has shown up
  as a perf symptom, so the per-lookup overhead must be measured.
- **Thread an explicit `AnalysisCache` through the hashing APIs.** Cleanest
  semantically; the complication is the same two-ASTs-at-once matching phase —
  callers must route each call to the correct AST's cache (thread both).

Either removes the whole bug class _and_ the residual prior-match slowdown, and
lets caches stay warm within an AST's life (the current resets throw away live
entries too, paying a recompute cost). It is a cross-cutting change to the
hottest code path and should be done as its own focused effort with perf
measured — NOT mid-walk (the walk executes main's working tree per hop).

## Reproduction (no LLM required)

- experiments/031-ephemeron-per-parse/ — parse-cycle and live-main-AST benches
  (parts 1–2). `bench-live-postnaming.mts --free-ast` shows the post-naming
  live-AST tax.
- experiments/032-prior-match-naming/bench.mts — the prior-match window on real
  archived 2.1.207/208 bundles; shows the 49 s vs 214 s dense-table penalty.
  Run with `NODE_OPTIONS="--max-old-space-size=14336 --expose-gc" npx tsx …`
  under a shell `timeout` (a quadratic cannot self-interrupt), sandbox disabled.
- Archived read-only 30 MB bundles:
  `~/Development/unpacked-claude-code-run-2026-07-17/versions/claude-code-2.1.<v>/.humanify/humanified.js`

## Diagnosis recipe

Hop wall-time > 30 min AND log silent AND worker ~100% CPU on one core AND
LLM-server GPUs idle → this issue. Confirm:
`sample $(pgrep -f 'tsx src/index' | tail -1) 4 | grep -cE 'Rehash|WeakCollection'`
(> 0 = the pathology). RSS is a tell: the big-AST thrash runs at 7–8 GB; a
light-heap CPU phase at ~2 GB is a different (benign) slow phase.
