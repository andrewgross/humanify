# 032 — the prior-match → naming ephemeron window (part 3 of the hang saga)

## What this fixes

`src/rename/plugin.ts` `createRenamePlugin`, gated on a `--prior-version`:

1. parse NEW bundle (funnel resets → fresh cache era) + `buildUnifiedGraph`
   → the naming-era AST + node caches, HELD LIVE for the whole pass;
2. prior-version matching parses the PRIOR bundle with
   `preserveAstCaches: true` (the funnel deliberately does NOT reset — the
   matcher reads hash/binding entries keyed by BOTH ASTs at once), builds a
   PRIOR `UnifiedGraph` (fills the 3 node-keyed WeakMaps + Babel's path cache
   with millions of PRIOR-AST keys), matches, then RETURNS — dropping the
   prior AST + graph. **Those millions of keys are now tombstones.**
3. naming runs its node-cache ops over the NEW AST. Bulk-inserting / reading
   through the tombstone-dense tables makes V8 re-hash the backing store on
   nearly every op → the O(n²) 100%-CPU / flat-RSS naming hang (production
   hop 2.1.107 sat ~40min at 100% CPU / flat 7.9GB, GPUs idle).

exp031 part 1 (the parse funnel) starts a fresh era at every big _parse_, and
exp031 part 2 (`releaseNamingAst`) sheds the live main AST before the
post-naming re-parses. Neither covers the gap **between** step 2 and step 3:
the prior AST is dropped there but nothing re-parses, so no funnel fires and
the tombstones sit in the tables the naming pass then fills.

**The fix (one gated call, `resetNodeCachesAfterPriorMatch`):**
`resetAnalysisNodeCaches()` + `clearBabelTraverseCache()` right after
prior-version matching returns, before `runRenamePass`. Gated on
`options.priorVersionCode` — with no prior nothing filled a prior era, so a
reset would only force naming to recompute cold for no benefit. The caches are
pure deterministic memoization (`node-caches.ts` doc, structural-hash.ts,
fingerprint-index.ts, function-fingerprint.ts), so a reset only forces
recompute-on-demand; the names already transferred onto the new AST and the
close-match context strings are not cache-derived and are unaffected.

Mirrors the existing pattern exactly: an AST was dropped → clear the node
caches before the next heavy phase.

## Mechanism bench (`bench.mts`, real archived ~32MB bundles)

new = 2.1.208 (32.3 MB, 85 811 graph nodes), prior = 2.1.207 (31.9 MB,
84 937 nodes), from the archived 2026-07-17 run. Each arm in its own process
(fresh heap), `NODE_OPTIONS="--max-old-space-size=14336 --expose-gc"`, under
shell `timeout`. `--expose-gc` forces the dropped prior AST to collect into
tombstones before the timed section, removing GC-timing nondeterminism.

### In-vivo confirmation of the pathology (same in both arms, from the setup)

The bench's own setup already demonstrates the dense-table insert cost, before
the fix even applies:

| build (identical work, ~85k nodes each)                                         | time      |
| ------------------------------------------------------------------------------- | --------- |
| `buildUnifiedGraph` NEW — into a **fresh** table                                | 49 s      |
| `buildUnifiedGraph` PRIOR — into the **NEW-filled** table (`preserveAstCaches`) | **214 s** |

Same operation, ~same node count, **4.4× slower** purely because the second
build inserts into a node cache already dense with the first build's keys
(plus its own transient tombstones). This is exactly the effect the naming
pass suffers against the prior's tombstones — and exactly what the reset
removes.

### The window itself — `--phase=insert` (the cleanest pathology)

After the prior is built + dropped + GC'd (→ tombstones), bulk-insert a fresh
batch of NEW-AST keys (`buildUnifiedGraph` over a second NEW parse). Parse cost
is identical across arms; the delta is the tombstone rehash overhead.

| arm                            | insert-stress (`buildUnifiedGraph` over fresh NEW) |
| ------------------------------ | -------------------------------------------------- |
| THRASH — no reset (tombstones) | _pending_                                          |
| FIX — reset first (fresh)      | _pending_                                          |

### `--phase=sig` (the task's literal naming-analog)

Repeated whole-Program `computeStructuralSignature` over the live NEW AST
(every identifier does a `bindingByIdentifierNode.get`; the traverse fills
Babel's path cache). Serialization-heavy, so the cache delta is a smaller
fraction per iteration.

| arm    | iter 1    | iter 2 | iter 3 | iter 4 |
| ------ | --------- | ------ | ------ | ------ |
| THRASH | _pending_ |        |        |        |
| FIX    | _pending_ |        |        |        |

## Correctness (the real gate)

`npm run check` **EXIT 0**: typecheck + prettier + biome + **1346 unit** + **33
fingerprint** tests, all green. `npx biome check src/rename/plugin.ts` clean —
the reset lives in a tiny helper (`resetNodeCachesAfterPriorMatch`), so
`createRenamePlugin`'s cognitive complexity is unchanged (≤15).

The load-bearing evidence that a mid-pipeline reset is _safe_ is
`src/rename/plugin-cross-version.test.ts`: 16 tests run the FULL plugin with
`priorVersionCode` set — so `resetNodeCachesAfterPriorMatch` executes its reset
in each — on a real Bun fixture pair plus hand-crafted exact/close/vote/
closure-capture/swap/chain/drift/re-decoration cases. Every one asserts the
output re-parses (`parseFailure === undefined`); the reconcile cases also
assert `semanticFailure === undefined`. All pass. Clearing Babel's path/scope
cache while the graph's NodePaths are live does not corrupt the rename
(`path.scope` is assigned on the path object and survives the cache clear;
`scope.rename` mutates the held scope directly), and resetting
`bindingByIdentifierNode` before naming leaves the pre-generate structural
invariant intact — `checkStructuralInvariant` recomputes the signature
self-containedly (`collectIdentifierBindings` repopulates before it reads), and
a pure rename preserves Babel Binding identity, so fresh resolution reproduces
the baseline signature (the cold-re-parse `checkResolvedSignature` already
relies on exactly this today).

## Risk not fully ruled out

The `preserveAstCaches` prior build is itself slow (214 s vs 49 s) for the same
reason — but that is INHERENT to cross-version matching (the matcher needs both
ASTs' warm entries) and PRE-EXISTING, not introduced or targeted here. This fix
addresses only the step-2 → step-3 window (naming), where nothing needs the
prior tombstones.
