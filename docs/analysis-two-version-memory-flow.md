# Memory-flow analysis: the two-version (--prior-version --split) pipeline

Date: 2026-07-20. Analyzed at main `6bbf122` (per-AST caches + re-export
relocation merged). Every claim below was verified against the code at the
cited line, not taken from comments or prior writeups — several existing
writeups turn out to contain wrong mechanisms (§5).

> **CORRECTION (later the same day, `c7b878e`):** this document's era model
> describes the INTENDED behavior, not what ran. `clearBabelTraverseCache()`
> was a silent no-op (the cache namespace hangs off the resolved traverse
> function — `ns.default.default.cache` under tsx — and the wrapper's probes
> missed it), so none of the "era" clears below actually happened before
> `c7b878e`: the process ran ONE Babel cache era for its whole lifetime,
> accumulating every AST's entries. I verified Babel's `clear()` replaces
> its maps (§1) but not that our wrapper _reached_ `clear()` — the missing
> verification. The §6 mechanism ranking is superseded: the observed pin is
> V8's at-capacity `EphemeronHashTable` Put path (full GC + in-place rehash
> per insert) on that one giant deleted-dense table; see
> docs/split-thrash-persists-after-b373a4c.md (status section) for the full
> corrected story. The live-set/retainer analysis (§2–§4) and the release
> fixes stand.

TL;DR:

- One hop performs **six to eight full parse+scope-crawl cycles of the same
  ~17–32 MB bundle**, each materializing a multi-GB NodePath/Scope graph into
  a fresh Babel cache era. Three windows hold **two full AST graphs at once**.
- The Babel-cache "era" hygiene is sound where it exists: `clear()` REPLACES
  the WeakMaps (tombstones cannot cross a clear), there is exactly one deduped
  `@babel/traverse@7.29.7`, and the split's own parse goes through the funnel
  clear. The 2026-07-20 hang writeup's central mechanism (naming-era
  tombstones surviving into the emit crawl) is therefore **not possible as
  described** (§5).
- The two mechanisms that DO remain and match the observed 2.1.172 hang
  signature: (a) the **bun-relink per-file churn loop** — ~1,500
  parse+traverse+scope-crawl cycles whose ASTs die immediately, all inserting
  into ONE cache era with no intermediate clears (the exact
  insert-storm-over-dying-keys shape that makes V8 rehash ephemeron tables on
  inserts), and (b) the **split-phase wrapper crawl running while the
  post-rename AST is still held live** (`releaseSplitSourceState` runs AFTER
  the emit, so the crawl's GC traffic traces both graphs). The observed flat
  ~2.4 GB RSS during the hang fits (a) better than (b) (§6).
- OOM inconsistency has a mundane driver: only `walk-versions.sh` raises the
  heap (`--max-old-space-size=14336`); the e2e harness and any direct
  `npx tsx src/index.ts` run at Node's default (~4 GB), which the dual-AST
  windows exceed on large bundles (§7).

## 1. Phase/era timeline (verified)

"Era" = one lifetime of Babel's module-level caches
(`@babel/traverse/lib/cache.js`: `pathsCache: WeakMap<parentNode,
Map<childNode, NodePath>>` + `scope: WeakMap<node, Scope>`).
`clearBabelTraverseCache()` (babel-utils.ts:50) REPLACES both WeakMaps, so an
era's entries die with the old map object — a cleared era leaves no
tombstones in the next one. Clears happen at exactly three places
(verified by grep, no others):

1. The parse funnel, for sources ≥ 5 MB (`maybeClearBabelCache`,
   babel-utils.ts:98; also `transformWithPlugins`, babel-utils.ts:163).
2. The prior-match → naming boundary (`clearBabelCacheAfterPriorMatch`,
   plugin.ts:592, called at plugin.ts:846).
3. The top of the Bun re-link (bun-relink.ts:196).

Key Babel mechanic (verified in 7.29.7): `scope.init()` → `crawl()` is
invoked from `path/context.js:122/139` — i.e. **any `traverse()` of a tree
crawls the scopes it passes and materializes+caches a NodePath for every
visited node**. A full-bundle traverse ≈ millions of `WeakMap.set` calls into
the current era.

One `--split --prior-version` hop, in order:

| #   | Phase                             | Parse of                                                                                                                  | Era                                                        | Big traversals filling the era                                                                                                                                                                                                                                                    | Dies at end?                                                                                                      |
| --- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| P0  | unpack (bun adapter)              | original bundle                                                                                                           | U (funnel clear)                                           | factory extraction traversals                                                                                                                                                                                                                                                     | yes                                                                                                               |
| P1  | beautify (`transformWithPlugins`) | unpacked source                                                                                                           | B (clear)                                                  | full transform traverse                                                                                                                                                                                                                                                           | yes                                                                                                               |
| P2  | rename parse + graph              | beautified code → **AST₁**                                                                                                | A (clear, plugin.ts:747)                                   | `buildUnifiedGraph` (plugin.ts:769), `captureSemanticBaseline` (780), taint, classification                                                                                                                                                                                       | no — naming needs it                                                                                              |
| P3  | prior match                       | prior humanified → **AST₂**                                                                                               | still A (**preserveAstCaches**, prior-version.ts:354)      | prior `buildUnifiedGraph` (prior-version.ts:158), both fingerprint indexes (288)                                                                                                                                                                                                  | AST₂ collectible on return (§3)                                                                                   |
| —   | boundary                          | —                                                                                                                         | A→C (clear, plugin.ts:846)                                 | —                                                                                                                                                                                                                                                                                 | AST₂'s era-A entries die with map                                                                                 |
| P4  | LLM naming + floor                | —                                                                                                                         | C                                                          | rename traversals over AST₁                                                                                                                                                                                                                                                       | —                                                                                                                 |
| P5  | structural invariant + generate   | —                                                                                                                         | C                                                          | `computeStructuralSignature` full traverse (plugin.ts:909), `generate` (916)                                                                                                                                                                                                      | —                                                                                                                 |
| —   | release                           | —                                                                                                                         | —                                                          | `releaseNamingAst` (plugin.ts:929): AST₁+graph freed (non-ledger mode)                                                                                                                                                                                                            | AST₁ dead                                                                                                         |
| P6  | validate                          | output.code → AST_v                                                                                                       | D (clear via funnel, output-validation.ts:217)             | `measureSemantics` + signature — full crawl                                                                                                                                                                                                                                       | yes                                                                                                               |
| P7  | reconcile                         | output.code → **AST₄**                                                                                                    | E (clear, reconcile-step.ts:49)                            | baseline+signature crawls, rename application; external `diff` for the text diff                                                                                                                                                                                                  | **kept** in outcome                                                                                               |
| P8  | deferred sweep                    | recon.code → **AST₅**                                                                                                     | F (clear, sweep-step.ts:52)                                | baseline+signature crawls, sweep                                                                                                                                                                                                                                                  | **kept**; AST₄ still live → **dual-AST window #2**                                                                |
| P9  | census + return                   | —                                                                                                                         | F                                                          | `collectMintedBindings(finalAst)`                                                                                                                                                                                                                                                 | plugin returns `{ code, ast: finalAst }`                                                                          |
| P10 | stable split                      | renameResult.code → **AST₆**                                                                                              | G (clear via funnel, stable-split.ts:680)                  | `findWrapperFunction` (682) → **full wrapper crawl happens HERE** (wrapper-detection.ts:104 reads `path.scope.bindings`); `statementHash` ×~24k (pure node walk, NO cache — statement-hash.ts); `assertConcatEquivalence` (718) → ~212 small parses (parse-only, no cache writes) | finalAst loses its cached paths at the G clear (good), but its **node graph stays live** → **dual-AST window #3** |
| P11 | runnable emit                     | reuses AST₆ (cjs-emit.ts:1229)                                                                                            | G                                                          | `getBinding`s hit the already-crawled scope (611/648/703); `planTopLevelThis` full wrapper traverse (974) — mostly cache hits                                                                                                                                                     | —                                                                                                                 |
| P12 | write + release                   | —                                                                                                                         | —                                                          | `releaseSplitSourceState` (unified.ts:526): AST₅+AST₆ dead                                                                                                                                                                                                                        | era G = millions of dead keys (inert until next insert into G — but see P13)                                      |
| P13 | bun re-link                       | ~1,256 factory files + ~212 split files, **each parsed twice** (bun-relink.ts:94 `factoryRefs`, 112 `headerInsertOffset`) | H (clear at loop start, 196) — **no clears between files** | per file: full traverse w/ Identifier visitor + `p.scope.getBinding` (101) → per-file crawl; file AST dies as loop advances                                                                                                                                                       | continuous key churn                                                                                              |
| P14 | using-desugar                     | every tree file (using-desugar.ts:81)                                                                                     | H                                                          | parse-only + `t.traverseFast` guard (no cache) unless the file has `using`                                                                                                                                                                                                        | small                                                                                                             |
| P15 | scaffold                          | —                                                                                                                         | —                                                          | regex scans only                                                                                                                                                                                                                                                                  | —                                                                                                                 |

## 2. The three dual-AST windows (the real peaks)

A fully crawled AST graph for a 17–32 MB beautified bundle (nodes + NodePaths

- Scopes + Bindings + cache Maps) is on the order of **1.5–4 GB** (the repo's
  own docs describe renameResult.ast as "multi-GB"; treat exact numbers as
  needing a heap-snapshot measurement).

1. **Prior match (P3):** AST₁ (crawled) + AST₂ (crawled) + two graphs + two
   fingerprint indexes. Inherent to in-process matching. This is the window
   the planned "prior FingerprintIndex in a subprocess" lever removes.
2. **Reconcile + sweep (P7–P9):** AST₄ is retained in the reconcile outcome
   (reconcile-step.ts:78) while the sweep parses AST₅; both stay live until
   the plugin returns (`recon` local, plugin.ts:948–977). Note the sweep only
   needs `recon.code` (a string) — AST₄ is held solely as a finalAst
   _fallback_ for the case where the sweep applies nothing, and
   `resolveFinalOutput` already has a re-parse fallback (plugin.ts:475).
3. **Split + emit (P10–P12):** `renameResult.ast` (finalAst) is deliberately
   held through the entire split+emit for the adapter-split fallback
   (unified.ts:582–601) and released only after the tree is written
   (unified.ts:526). So the biggest single-era cache fill of the run (the
   wrapper crawl) executes while a second full graph is live. On the walk's
   Bun bundles the fallback never runs (wrapper input), so this window buys
   nothing in practice.

Strings are secondary but add up: `original.source` (retained through
runSplit for the fallback, unified.ts:768/811), `renameResult.code`,
`priorVersionCode` (collectible once `unminify` returns — verified the plugin
closure is unreachable by then), output/recon/sweep code strings (transient),
both tree maps (`fileContents` + `runnable`, ~2× tree text), and the
reconcile diff text (spawnSync stdout, capped at **512 MB** —
diff-reconcile.ts:147; a format-divergent prior can produce a diff text of
~2× file size, a real transient spike).

## 3. What does NOT leak (verified retainer audit)

- `RenameReport`/coverage/transferStats are strings+counters (types.ts:286) —
  keeping them after `releaseNamingAst` pins nothing.
- Prior-side objects do not escape the matcher: `CloseMatchInfo` carries
  prior CODE strings and **new-side** Bindings (the `TransferPair.binding`
  resolved from `pair.next.bindings` in statement-align); `MatchResult` is
  sessionId strings. AST₂ is collectible when `matchPriorVersion` returns.
- The per-AST `AnalysisCache` registry is `WeakMap<rootNode, AnalysisCache>`
  (analysis-cache.ts:51) — one entry per tree, plain strong Maps inside that
  die with the tree. No module-level node-keyed table remains in src/ (the
  analysis-cache design goal holds).
- Exactly one `@babel/traverse` (7.29.7, fully deduped — npm ls verified), so
  clears cannot hit a "wrong copy".
- Ledger mode (`--rename-ledger`) intentionally keeps AST₁ through the
  post passes (plugin.ts:446–453) → THREE full graphs; the walk does not pass
  it (walk-versions.sh:118), but don't combine it with big bundles casually.

## 4. Cost model even when nothing "leaks"

Each era's first full traverse re-pays parse + full scope crawl + path
materialization for the same bundle. Per hop that is ~6–8 full cycles: P2,
P3(prior), P6, P7, P8, P10 (+P0/P1 on the original text). The plugin comment
at plugin.ts:919 records the measured symptom: each post-naming pass ran
45–53 s instead of 8–15 s when the naming-era graph was still live —
i.e. the dominant term is **GC tracing of whatever ELSE is live during a
crawl**, not the crawl itself. That is why release points (what is live
_during_ each crawl) matter more than clear points.

## 5. Review of docs/issue-split-emit-scope-crawl-ephemeron-thrash.md

Verified TRUE:

- `resetAnalysisNodeCaches` is gone (0 definitions/call sites).
- Clear sites are exactly the three listed (§1).
- Babel does lazily crawl on first scope use; `relocateNamespaceAugmentations`
  does call `scope.getBinding` (cjs-emit.ts:611/648).
- The hang signature capture (WeakMapPrototypeSet → EphemeronHashTable::Rehash)
  and the "use the sample count, not RSS" advice are good.

REFUTED (mechanism, not symptom):

1. _"Nothing clears a node-keyed cache between the naming phase and this
   crawl"_ — _stableSplitFromCode's own parse_ goes through the funnel
   (`parseFileAst` → `parseSourceAst` → `maybeClearBabelCache`,
   stable-split.ts:680; the bundle is ≥5 MB), which **replaces** both
   WeakMaps at the top of the split. Naming-era keys cannot be tombstones in
   the era the emit inserts into. (They also aren't "uncleaned": clear() drops
   the whole map object; V8 collects it wholesale.)
2. _"The first `getBinding` on an un-crawled wrapper scope happens in
   `relocateNamespaceAugmentations`"_ — the full wrapper crawl happens
   **earlier, inside stableSplitFromCode**, when `findWrapperFunction` reads
   `path.scope.bindings` (wrapper-detection.ts:104) during a traverse whose
   `setContext` inits scopes (context.js:122). By the emit, the scope is
   crawled and `getBinding` is a lookup. The relocation function is not the
   crawl trigger, and moving it to ledger-based lookup (their fix A) does not
   remove the crawl — `buildPlan` constitutively needs `scope.bindings` and
   `binding.referencePaths` (cjs-emit.ts:703, 516–533).
3. Consequently their fix (B) ("clear at the top of emitRunnableCjs") is a
   no-op for the theorized mechanism: the era at that point is already fresh
   (started at the split parse) and filled only with LIVE wrapper keys. Their
   decisive test would come back "still thrashes" and mis-point to the
   per-AST cache layer (their C), which §3 shows is structurally clean.
4. The `4dbfcbc` split-boundary reset the per-AST swap deleted was, for
   Babel's cache, REDUNDANT with the funnel clear at the split parse — its
   deletion did not remove a Babel-cache clear from this path.

## 6. So what DID hang 2.1.172? Two candidate mechanisms that fit

The log freeze point ("Split ledger: inheriting assignments…") starts a
window with **no progress output at all** until the relink message: split
parse+crawl, statement hashing, equivalence check, emit, tree write, release,
and the entire relink loop are all silent (unified.ts:489–533,
bun-relink.ts:186). A slow phase in ANY of them looks like the same frozen
log. Within that window:

**(a) Relink per-file churn (best fit for the observed capture).**
~1,500 files × (2 parses + full traverse + per-file scope crawls) into one
era with no intermediate clears. Each file's keys die as the loop advances,
so the ephemeron table runs under sustained insertion with continuously
regenerating dead entries — precisely the insert-path
`WeakCollectionSet → Rehash` regime, and severity depends on GC timing
(matches the 172-hit/174-miss nondeterminism). RSS during this phase is
strings + one file's AST + the cache table — **matches the observed flat
~2.4 GB**. The pre-existing clear at bun-relink.ts:196 protects against the
_previous_ era's tombstones but does nothing about churn _within_ the loop.

**(b) Split-phase wrapper crawl under a big live set.** The
`findWrapperFunction` crawl (millions of inserts + GBs of path/scope
allocation) runs while finalAst's full node graph is still held
(release happens at P12, after emit). Every GC the crawl triggers traces
both graphs — the same class the repo already measured at the naming
boundary (plugin.ts:919). This predicts a _slow_ phase (minutes, scaling
with bundle size), 99% CPU, but RSS well above 4 GB — so the 2.4 GB capture
argues it was not (b) that day. (b) is still the main structural cost of the
split phase.

Decisive next-hang discriminator (no code archaeology): add phase-stamped
debug lines (split: parsed / wrapper-crawled / hashed / equivalence-checked /
emitted / written / released; relink: every 100 files). The next hang's log
then names its phase directly, and RSS separates (a) from (b).

## 7. Why "memory issues" appear inconsistently across runs

- Only the walk driver raises the heap: `NODE_OPTIONS=--max-old-space-size=14336`
  (walk-versions.sh:118). `npm run e2e -- validate`, the fptests, and any
  direct `npx tsx src/index.ts` run at Node's default (~4 GB). The dual-AST
  windows on 26–32 MB bundles plausibly exceed 4 GB — consistent with the
  e2e 207→208 leg OOMing while walk hops pass. Same pipeline, different
  ceiling.
- Within one configuration, the remaining variance is GC-timing-dependent
  (mechanism (a)) and bundle-size-dependent (windows #2/#3 scale with the
  release), which is why cache-clearing changes never made failures
  _consistently_ disappear: the clears were already correct where they
  existed; the load is live-set overlap + per-file churn, not stale caches.

## 8. Recommended fixes, ranked

1. **Era hygiene inside the relink loop** (walk-unblocking, ~3 lines):
   in `relinkBunModules`, call `clearBabelTraverseCache()` every N files
   (e.g. 100) in both loops. Entries are per-file and useless across files;
   this bounds the ephemeron table at N files' worth. Optional: reuse the
   `factoryRefs` parse in `headerInsertOffset` (each file is parsed twice
   today, bun-relink.ts:94/112).
2. **Release the post-rename AST BEFORE the split parse**, not after the
   emit: on the stable-split path the only consumer of `renameResult.ast` is
   the adapter fallback (unified.ts:593), which (i) never runs for wrapper
   bundles and (ii) can re-parse `original.source` on the rare path where it
   does. Nulling `renameResult.ast` at `runSplit` entry removes dual-AST
   window #3 and most of the split phase's GC tax. (Keep `stable.wrapper`
   release where it is.)
3. **Drop `recon.ast` before the deferred sweep** in non-ledger mode: the
   sweep consumes `recon.code` (a string); `resolveFinalOutput` already
   re-parses when no AST is on hand (plugin.ts:475). Removes dual-AST
   window #2 for the cost of one re-parse only when the sweep applies
   nothing.
4. **Prior FingerprintIndex in a subprocess** (already the planned lever, per
   experiments/032): removes window #1, the largest inherent peak, and all
   prior-era cache interplay.
5. **Heap-limit parity**: set the same `--max-old-space-size` (or
   `NODE_OPTIONS`) in the e2e harness/package scripts as the walk, or accept
   1–3 first and lower the walk's 14 GB.
6. **Observability**: the §6 phase-stamped log lines; they are the cheap
   permanent insurance against the next silent-window misdiagnosis.

Explicitly NOT recommended: the hang writeup's (B) (clear at the top of
`emitRunnableCjs`) — the era there is already fresh and the crawl it targets
happened earlier and is required; and (A) (ledger-based lookup in
`relocateNamespaceAugmentations`) as a _thrash_ fix — fine as a cleanup, but
`buildPlan` still crawls.

## 9. Pointers

- Parent docs: docs/issue-ephemeron-cache-thrash.md,
  docs/issue-split-emit-scope-crawl-ephemeron-thrash.md (mechanism corrected
  here, §5–6), docs/live-test-per-ast-canary.md.
- Babel internals verified: node_modules/@babel/traverse/lib/cache.js
  (map replacement on clear; WeakMap<parent, Map<child, NodePath>>),
  scope/index.js:658 (lazy init→crawl), path/context.js:122 (init on visit).
