# Issue: split/emit ephemeron thrash — `relocateNamespaceAugmentations` force-crawls the wrapper scope into a tombstone-dense WeakMap

Status: **MITIGATIONS LANDED 2026-07-20 — and the mechanism below is
CORRECTED by docs/analysis-two-version-memory-flow.md (§5–6).** Line-level
verification refuted this doc's root vector: the split's own parse goes
through the ≥5 MB funnel, which REPLACES Babel's cache WeakMaps
(babel-utils.ts:98 + cache.js `clear()`), so naming-era tombstones cannot be
in the table the emit inserts into; and the full wrapper crawl happens at the
TOP of `stableSplitFromCode` (`findWrapperFunction` reads
`path.scope.bindings`), not first in `relocateNamespaceAugmentations` — by
emit time `getBinding` is a lookup. The candidate fixes below ((A)/(B)) would
therefore not have fixed the hang. Best-fit mechanisms for the observed
signature (the log is silent from "Split ledger:" through the relink, so the
freeze point never localized the phase): the bun-relink per-file churn loop
(~1,500 parse+traverse cycles into ONE era, dead keys regenerating under
continuous insertion), and secondarily the split crawl running while the
post-rename AST was still held. Landed: periodic era resets + single parse
per file in the relink loop, post-rename AST released before the split
parse, recon-AST released before the deferred sweep, and phase-stamped debug
logs through the whole silent window. Original (superseded) analysis kept
below for the record.

Original status: **OPEN, walk-blocking.** The 2.1.172 rebuild hangs in the split/emit
phase with the O(n²) V8 ephemeron-rehash signature. Root vector: the per-AST
cache swap (`7b55f81`) deleted the `4dbfcbc` split-boundary cache reset, so the
split/emit's wrapper-scope crawl now lands on a tombstone-dense node-keyed
WeakMap. Nondeterministic (2.1.174 dodged it on its run); 2.1.172 hit it. The
boot fix's `relocateNamespaceAugmentations` sharpens the trigger but does not
cause it.

Found 2026-07-20 during RUN 4, on the very first hop (2.1.172) that exercises
both changes end-to-end.

## TL;DR

`relocateNamespaceAugmentations` (src/split/cjs-emit.ts:635, the boot fix) calls
`scope.getBinding(...)` on `wrapper.scope` — the scope of the entire ~16.8 MB
bundle wrapper (cjs-emit.ts:611 and :648). The **first** `getBinding` on an
un-crawled scope makes Babel **crawl the whole wrapper**, inserting **millions**
of node keys into a node-keyed WeakMap. At that point in the pipeline the target
table is **tombstone-dense** (keys from the dropped naming-era AST that nothing
cleared between naming and the emit), so V8 re-hashes the table on nearly every
insert → `WeakMapPrototypeSet → WeakCollectionSet → EphemeronHashTable::Rehash`,
99% CPU on one core, log frozen. Same O(n²) class as
docs/issue-ephemeron-cache-thrash.md — one phase later, from a **new** trigger.

This is the **residual nondeterministic** ephemeron thrash (documented as open in
docs/issue-ephemeron-cache-thrash.md) landing on the emit-phase wrapper-scope
crawl. It is NOT uniquely the boot fix: 174's emit also operates on
`wrapper.scope` (buildPlan/planWrapperContext), and 2.1.174 built clean in ~9 min
— it _dodged_ the thrash on that run (favorable GC/tombstone timing), where 172
hit it. The reliable regression vector is that the **per-AST swap deleted the
`4dbfcbc` split-boundary reset** (`resetAnalysisNodeCaches` + its
`clearBabelTraverseCache`) that used to clear caches right before the split,
keeping tombstone density low at the emit crawl. The boot fix's
`relocateNamespaceAugmentations` moves the _first_ `wrapper.scope.getBinding`
slightly earlier but is not the root cause.

## Symptom (captured live)

- Hop 2.1.172 rebuild, worker pinned **99.3% CPU**, RSS **~2.4 GB**, wall-time
  41 min and climbing; log frozen right after
  `Split ledger: inheriting assignments from …/2.1.170/.humanify/split-ledger.json`
  (naming had finished in 4m39s; all the extra time is the split/emit).
- `sample <pid>` heaviest stack (leaf → caller):
  ```
  EphemeronHashTable::Rehash
  JSWeakCollection::Set
  Runtime_WeakCollectionSet
  Builtins_WeakMapPrototypeSet          ← a WeakMap.set (not WeakSet)
  Builtins_InterpreterEntryTrampoline × N   (JIT'd JS — unsymbolicated)
  AsyncFunctionAwaitResolveClosure … RunMicrotasks   (async emit continuation)
  ```
  `sample <pid> 2 | grep -cE 'Rehash|WeakCollection'` → 8 (>0 = the pathology).

**RSS heuristic caveat:** the earlier diagnosis recipe said "7–8 GB = real
thrash, ~2 GB = benign CPU phase." This thrash ran at **2.4 GB** and was real
(Rehash confirmed). The Babel scope-crawl fill doesn't need a huge heap. **Use
the `Rehash`/`WeakCollection` sample count, not RSS, as the tell here.**

## Scope / blast radius

- Blocks the whole **172→211** stretch: every version from 2.1.172 carries the
  teammates feature, so every hop runs `relocateNamespaceAugmentations` on its
  wrapper scope and is at the same risk. The walk is **paused** at this finding
  (last clean/booting version: 2.1.170).
- Retrying the hop _might_ clear it (the thrash is nondeterministic — 174 dodged
  it), but with ~40 hops (172→211) each rolling the same dice, retry-roulette is
  not acceptable — this needs a code fix.

## Root cause (confirmed mechanism + one open detail)

Confirmed:

1. `emitRunnableCjs` calls `relocateNamespaceAugmentations(statements, order,
wrapper.scope)` (cjs-emit.ts:1239). To find each augmentation target's
   declaration it calls `scope.getBinding(name)` on the **wrapper scope**
   (cjs-emit.ts:611, :648). Babel lazily **crawls** an un-crawled scope on first
   `getBinding`, walking the entire wrapper subtree and creating/caching a
   NodePath per node in Babel's module-level node-keyed cache.
2. Nothing clears a node-keyed cache between the naming phase and this crawl.
   `clearBabelTraverseCache()` is called only at: the ≥5 MB parse funnel
   (babel-utils.ts:100), the Bun relink (bun-relink.ts:196), and the prior-match
   boundary _before_ naming (plugin.ts:596). `resetAnalysisNodeCaches()` — the
   `4dbfcbc` reset at the top of the split that used to clear here — was
   **deleted** by the per-AST swap (0 call sites remain). So the naming-era AST's
   dead keys are still tombstones in the table when the crawl bulk-inserts.
3. Both 172 and 174 crawl `wrapper.scope` in the emit (`buildPlan` and
   `planWrapperContext` at cjs-emit.ts:1246/:1252 both take it), so the crawl is
   not unique to the boot fix — 174 simply dodged the thrash on its run. The
   differentiator is tombstone density at crawl time: raised by the removed
   split-boundary reset (point 2) and varying with GC timing (the documented
   nondeterminism). `relocateNamespaceAugmentations` only moves the _first_
   `getBinding` earlier in the emit.

Open detail (which WeakMap): almost certainly **Babel's internal traverse/scope
cache** (the crawl is pure Babel), but the per-AST `AnalysisCache` node→root
resolution should be ruled out too — if it memoizes node→root or node→cache in a
module-level `WeakMap<node, …>`, statement hashing during the split would fill it
the same way. The decisive test below distinguishes them.

## Decisive test (≈10 min, no code archaeology)

Re-run the 2.1.172 hop with a `clearBabelTraverseCache()` added immediately
before the scope work in `emitRunnableCjs` (right before
`relocateNamespaceAugmentations` / the first `wrapper.scope.getBinding`):

- **Thrash disappears →** it is Babel's cache; pick a fix from (A)/(B) below.
- **Still thrashes →** it is a humanify `WeakMap<node,…>` in the per-AST cache
  layer; instrument `src/analysis/analysis-cache.ts`'s node→root/entry
  resolution and scope that map per-AST (it must die with its AST, not persist
  module-level).

Reproduce the hop directly (no walk driver needed):

```bash
cd /Users/andrewgross/Development/humanify
npx tsx src/index.ts \
  /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.172/binary-decompiled/src/entrypoints/index.js \
  --split --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b \
  --api-key local --reasoning-effort low -c 32 \
  -o /tmp/emit-thrash-172 \
  --prior-version /Users/andrewgross/Development/unpacked-claude-code/versions/claude-code-2.1.170/.humanify/humanified.js \
  -vv --log-file /tmp/walk-172.log
# hangs after "inheriting assignments"; confirm with:
sample "$(pgrep -f 'claude-code-2.1.172/binary' | tail -1)" 3 | grep -cE 'Rehash|WeakCollection'
```

Naming (~5 min, needs the LLM endpoint) precedes the crash; to iterate faster on
just the emit, cache a beautified+named wrapper once and re-run the split alone
if the split harness supports it.

## Candidate fixes (ranked)

**(A) Don't crawl the wrapper at all — use the ledger.** `relocateNamespace‑
Augmentations` only needs each target binding's **declaration site** (which file
/ statement declares `targetName`). The stable-split **ledger already maps every
top-level binding to its declaring file/statement** — that is the split's core
data structure. Resolve `targetName` from the ledger/plan instead of
`scope.getBinding`, and the expensive Babel scope crawl never happens. Removes
the cost, not just the thrash; no dependence on cache-clear ordering. Preferred.

**(B) Clear the node cache at the emit boundary.** Call
`clearBabelTraverseCache()` (and, if the per-AST layer keeps any module-level
node map, its reset) once at the top of `emitRunnableCjs`, before the scope
work — restoring what the deleted `4dbfcbc` split reset did. Cheap and localized,
but pays a full re-crawl and re-introduces boundary-reset bookkeeping the per-AST
swap was trying to retire.

**(C) Per-AST-scope any humanify node map** implicated by the decisive test, so
it dies with its AST (consistent with the per-AST design intent).

(A) is the most robust; (B) is the fastest unblock if a rebuild is urgent. They
compose — (B) to unblock the walk now, (A) as the durable fix.

## Related

- `docs/issue-ephemeron-cache-thrash.md` — the parent O(n²) ephemeron class and
  the per-AST swap that was supposed to retire it (it retired the _analysis_
  maps; this is a Babel-cache/emit-phase gap the swap's boundary-reset deletion
  re-exposed).
- `docs/issue-runnable-boot-foreign-namespace-reexport.md` — the boot fix
  (`relocateNamespaceAugmentations`) whose scope crawl is the new trigger. Its
  correctness is fine; only its _scope-lookup mechanism_ is the perf problem.
- `experiments/032-prior-match-naming/` — benches for the two-AST live-heap cost;
  the same `sample`/`--expose-gc` recipe applies.
- Walk runbook / restart: `unpacked-claude-code/NEXT-STEPS-2026-07-20.md`,
  `unpacked-claude-code/RUNNER.md`. Walk is paused; 172 partial output removed.
