# Status: the 2.1.172 split hang PERSISTS after b373a4c — consolidated analysis + issue

Status: **ROOT CAUSE FOUND AND FIXED 2026-07-20 (`c7b878e`).**
`clearBabelTraverseCache()` was a **silent no-op for the project's entire
history**: @babel/traverse attaches its cache namespace to the traverse
FUNCTION (`traverse.cache = cache`), which under tsx interop lives at
`ns.default.default.cache`; the wrapper probed `ns.cache` and
`ns.default.cache` (both undefined) and optional-chained into silence
(empirically probed: `clear actually swapped the cache map: false`). So every
"era boundary" (parse funnel, prior→naming boundary, relink clears — and
b373a4c's periodic relink clears) never cleared anything, and Babel's
node-keyed WeakMaps accumulated EVERY AST of the whole run. Once that
EphemeronHashTable is huge and deleted-dense, V8's at-capacity `Put` path
(full GC + in-place Rehash per insert) produces exactly this doc's captured
signature, nondeterministically by GC timing — which is why one rebuild
pinned while an identical run 25 min earlier sailed through, and why every
past clear-based fix behaved inconsistently while the AST releases (real
object-graph changes) held. Corrections to this doc's specifics: the hang
sample's process had a **9.6 GB footprint (peak 10.4 GB)** — the ~2 GB RSS
reading was from the wrong process, and heap-flag theories are moot; the
"statement hashing through the analysis layer" hypothesis is refuted
(statement-hash.ts is a pure node walk, no caches, and `AnalysisCache` has
one root-keyed entry per tree); and the freeze point cannot distinguish
parse/crawl/hash from the emit — everything from the sweep's last LLM
response through the tree write runs as ONE synchronous microtask
continuation (the capture shows the pin resumed from a socket read), during
which debug WriteStream stamps cannot flush. Fix: resolve the cache off the
RESOLVED traverse function and THROW if absent; red/green test pins the
behavior. With working clears the standalone split repro drops 18.6s → 7.3s.
Original text kept below for the record.

Original status: **OPEN, walk-blocking.** RUN 4 is paused at 2.1.172 (last clean/booting
version 2.1.170). The three landed fixes (per-AST caches, the boot fix, the
split→relink churn bound) did **not** resolve the ephemeron thrash; a fresh
rebuild on `b373a4c` hung identically. New evidence localizes the hang to the
**first split sub-phase (parse → wrapper crawl → statement hashing)**, which is
_not_ where `b373a4c` applied its fix.

This doc supersedes the tangle of prior notes for handoff. Background/history:
`issue-split-emit-scope-crawl-ephemeron-thrash.md` (my first, partly-wrong
analysis + the reopened note), `analysis-two-version-memory-flow.md` (the other
agent's memory-flow analysis + §8 fix list), `issue-ephemeron-cache-thrash.md`
(the parent O(n²) class). Written 2026-07-20 by the runner.

## Executive summary

- **What's the bug:** a hop hangs at 99% CPU, one core, log frozen, with the V8
  ephemeron signature `WeakMapPrototypeSet → WeakCollectionSet →
EphemeronHashTable::Rehash` (O(n²) rehash of a tombstone-dense node-keyed
  WeakMap). Confirmed on the 2.1.172 rebuild: `sample` count 9, 3728 samples in
  `Rehash`, ~30 min, **~2.0 GB RSS**.
- **Heap is NOT the lever.** RSS was ~2 GB against a 14 GB limit — this is not
  OOM. The walk already runs every hop at `--max-old-space-size=14336`
  (walk-versions.sh:118, one process for all stages). The memory-flow doc's heap
  item (§8 #5) is _parity for non-walk entry points_ and even suggests _lowering_
  the walk's heap — a bigger heap makes the tombstone table larger and the rehash
  worse. Do not chase heap for this.
- **Where it actually hangs (new):** the **earliest** split sub-phase, before
  assignments resolve — `stableSplitFromCode` at stable-split.ts:684–~701. The
  fixes in `b373a4c` targeted the **relink loop** (the _last_ sub-phase) and AST
  releases; the freeze is upstream of both, so they couldn't have fixed it.
- **Leading hypothesis:** `body.map(statementHash)` thrashes on a module-level
  node-keyed WeakMap in the **per-AST `AnalysisCache`** node→root/entry
  resolution, tombstone-dense from the naming phase's own hashing. Alternatives:
  the full-bundle parse or the wrapper scope crawl. A 2-line flush + rerun
  pinpoints which (below).

## What landed, and what each addressed (fix timeline)

All on `main`, stacked on the `ade8eae` the run started from:

| commit    | change                                                                                                                                                                         | targets                                          |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------ |
| `7b55f81` | per-AST `AnalysisCache` replaces the 3 module WeakMaps; deletes `resetAnalysisNodeCaches`                                                                                      | the _analysis_ maps' tombstones                  |
| `6bbf122` | `relocateNamespaceAugmentations` (boot fix)                                                                                                                                    | 2.1.172+ Bun boot crash (unrelated to this hang) |
| `b373a4c` | §8 #1 era hygiene in the **relink loop** + single parse per file; #2 release post-rename AST before split parse; #3 drop `recon.ast` before sweep; #6 phase-stamped split logs | the relink loop + dual-AST GC tax                |

`b373a4c` implemented memory-flow §8 items 1–3 + 6. **The hang survived all of
them.** Item #2 (release the post-rename AST before the split parse) is the only
one aimed at the hang's actual window — and it did not fix it, so either the
release is ineffective (AST still reachable) or the thrash is not the held-AST.

## Symptom, captured live (2.1.172 rebuild on b373a4c)

- Worker 100% CPU / **~2.0 GB RSS** / ~30 min; killed (a quadratic won't
  self-interrupt).
- `sample <pid> 5`: heaviest stack is `Builtins_WeakMapPrototypeSet →
WeakCollectionSet → JSWeakCollection::Set → EphemeronHashTable::Rehash` (3728
  samples in `Rehash`); JS callers are JIT'd/unsymbolicated.
- **stdout** frozen at `Split ledger: inheriting assignments from …/2.1.170/…`
  (unified.ts:295 — a light ledger read, printed at split entry).
- **debug `--log-file`** (3.29 M lines under `-vv`): last phase marker
  `[DEBUG:reconcile-prior-diff]`; **none** of the split phase-stamps appeared.
- Reproduces reliably: two full 2.1.172 rebuilds (pre- and post-`b373a4c`) both
  hung ~30 min at the same point — not bad luck.

## Localization (the new, load-bearing finding)

Pipeline order around the freeze:

1. Naming → post-generate `runPriorDiffReconciliation` → logs
   `[DEBUG:reconcile-prior-diff]` (plugin.ts:378) → deferred sweep → "Done!".
2. Split entry: `loadSplitLedgerIfPresent` prints **"Split ledger: inheriting
   assignments"** (unified.ts:295).
3. `stableSplitFromCode` (stable-split.ts) runs, logging in order:
   `parsing N byte bundle` (**684**) → _`parseFileAst(code)`_ (685) →
   _`findWrapperFunction` scope crawl_ → `wrapper crawled` (**696**) →
   _`body.map(statementHash)`_ → `statement hashes computed` (**~701**) →
   _assign_ → `assignments resolved` (**728**).
4. Then emit (cjs-emit.ts:1259) → relink (bun-relink.ts:235 — where `b373a4c`'s
   era-reset fix lives).

`reconcile-prior-diff` (step 1) reached disk; **no step-3 stamp did.** So the
hang is between "Split ledger:" and the first stamp that would have flushed —
i.e. inside the parse / crawl / statement-hash of `stableSplitFromCode`, well
**before** emit and relink. (Caveat: if `debug.log` buffers, one or two of
684/696/701 may have executed without flushing, which only moves the point
_within_ parse→crawl→hash — still upstream of the fix.)

### Why each stage is/ isn't suspect

- **`parseFileAst(code)` (685):** parsing builds a fresh AST and funnels through
  the ≥5 MB cache-clear; it does not bulk-insert into node-keyed WeakMaps. Low.
- **`findWrapperFunction` crawl:** fills Babel's path/scope cache — but the parse
  immediately preceding it just _cleared_ that cache (funnel), so the fill should
  be O(n) on a fresh table. Medium, only if something re-densifies it.
- **`body.map(statementHash)`:** hashes every top-level statement through the
  analysis layer, which since `7b55f81` resolves each node to its AST-root
  `AnalysisCache`. If that resolution memoizes node→root (or node→entry) in a
  **module-level `WeakMap<node,…>`**, this is a millions-key insert into a table
  the naming phase already filled-and-dropped → the exact O(n²). **Highest** —
  it is the per-AST swap's own hot path, untouched by all three fixes.

## Decisive next steps (ranked)

1. **Pinpoint the op (≈10 min):** make the four `stableSplitFromCode` stamps
   flush synchronously (or emit them to `process.stderr.write` which is
   unbuffered) and rerun the 2.1.172 repro. The last stamp before the freeze
   names the culprit: after `parsing…` = parse; after `wrapper crawled` =
   statement hashing; no stamp = the parse itself. This ends the guessing the
   phase-stamps were supposed to end (they currently don't flush in a CPU-bound
   hang).
2. **If it's statement hashing:** instrument `src/analysis/analysis-cache.ts`'s
   node→root/entry resolution for a module-level `WeakMap<node,…>` and scope it
   per-AST (die-with-AST), or clear it at the naming→split boundary. This is the
   per-AST swap's residual and the most likely fix.
3. **If it's the crawl:** something re-densifies Babel's cache between the parse
   and the crawl — audit for a node-keyed insert there; a `clearBabelTraverse‑
Cache()` immediately before `findWrapperFunction` is the targeted guard.
4. **Structural (memory-flow §8 #4):** run the prior FingerprintIndex / the
   two-AST work in a subprocess — removes the densest window and its cross-era
   cache interplay wholesale; the durable direction if point-fixes keep missing.

Not the lever: raising heap (§5) — it's ~2 GB, not OOM; and the relink-loop
era-reset (already landed) — the hang is upstream of the relink.

## Operational state / reproduction

- Walk **paused**; partial 2.1.172 removed; `main` at `b373a4c`; nothing running
  or armed to relaunch. Resume = RUNNER.md / NEXT-STEPS-2026-07-20.md sequence
  once a fix lands (172→211 all carry the teammates code and are all at risk).
- Repro (no walk driver; naming ~5 min needs the LLM endpoint):
  ```bash
  cd /Users/andrewgross/Development/humanify
  npx tsx src/index.ts \
    /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.172/binary-decompiled/src/entrypoints/index.js \
    --split --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b \
    --api-key local --reasoning-effort low -c 32 -o /tmp/thrash-172 \
    --prior-version /Users/andrewgross/Development/unpacked-claude-code/versions/claude-code-2.1.170/.humanify/humanified.js \
    -vv --log-file /tmp/w172.log
  # hangs after "Split ledger:"; confirm:
  sample "$(pgrep -f 'claude-code-2.1.172/binary' | tail -1)" 3 | grep -cE 'Rehash|WeakCollection'
  ```
  Freshest live capture kept at `scratchpad/sample-172-attempt3.txt`.

## Pointers

- `src/split/stable-split.ts:684–730` — the first split sub-phase + its stamps.
- `src/analysis/analysis-cache.ts` — the per-AST cache to audit for a
  node-keyed memo (hypothesis 2/step 2).
- `src/commands/unified.ts:295` (split entry), `:513` (`stableSplitFromCode`
  call), `:412`/`bun-relink.ts` (relink — where `b373a4c` fixed, downstream of
  the hang).
- `analysis-two-version-memory-flow.md` §5–8 — the memory-flow model + the fix
  list `b373a4c` drew from; this doc updates its §6 conclusion (the relink loop
  was not the site).
