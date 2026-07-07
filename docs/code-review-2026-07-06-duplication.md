# Code review — duplication, redundancy, and divergence (2026-07-06)

Focus per request: **duplicate code paths for similar things, redundant
code paths, and bugs introduced by having similar-but-not-quite-the-same
logic in many places.** This is a follow-up to `code-review-2026-07-04.md`
(whose C/D/I findings became plan items 1–5, all landed) and to the
lifecycle/config/collector unification just completed.

Every finding below was verified by reading the cited sites. Ordered as a
worklist: **A = can cause wrong output**, **B = redundant/degraded**,
**C = pure duplication (house-rule cleanup)**. Confidence and blast radius
noted per item.

**Status (2026-07-07):** A1, A2, and B3 are **fixed** — commits `78828d3`,
`9ff9205`, `b8680de` (B3 chose the "restore the retry loop" direction). Split
findings (A3, A4, B2, C1–C3, C8) are **deferred** — file-splitting work is not
the current focus. Everything else remains open.

Method: three subsystem sweeps (split; detection/unpack/library-detection;
llm/commands/coverage) plus a direct review of rename/analysis/prior-version.
No inline `TODO`/`FIXME`/`HACK` markers exist in `src` — the only hits are
vendored fixtures (zod/preact/nanoid) and spec placeholders (`TBD` for CLI
defaults in `docs/spec/`). So this doc IS the outstanding-work list.

---

## A. Divergences that can produce wrong output

### A1. [FIXED · 78828d3] Minifier detection labels almost everything `terser`; no swc detector exists

**`src/detection/signals/minifier.ts` + `src/detection/detect.ts:55-59`. High confidence, medium-high severity.**

`detectTerser` claims tokens every modern minifier emits — `void 0`
(line 11), `!0`/`!1` (line 20) — at `"likely"` tier. `detectMinifier`
(line 78) lists terser first, and `detect.ts:58` selects
`minifierSignals[0]` with **no tier discrimination among minifiers**. So
any minified bundle containing `void 0` (i.e. all of them) is classified
`terser`; `detectEsbuildMinifier`/`detectBunMinifier` can essentially
never win selection. There is **no swc minifier detector at all**, yet
`src/rename/skip-list.ts:86` has a `minifierType === "swc"` branch — so
swc bundles get labeled terser and their helper names
(`_interop_require_default`, etc.) are never protected from renaming.
`detect.test.ts` never asserts `minifier.type`, so this is untested.

Fix: make terser markers exclusive (or lower their tier / require
corroboration), add an swc detector, and sort minifier signals by tier
instead of array position. Note: the 119→120 run passes `--minifier bun`
explicitly, so that run is unaffected — this bites default CLI usage.

### A2. [FIXED · 9ff9205] Module-binding rename application desyncs `usedNames`/report from the AST

**`src/rename/processor.ts:984-991` vs `:446-447`; `applyValidRenames` at `:2041-2043`. High confidence; latent on current targets.**

The function-path apply falls back when the fast path no-ops:

```ts
if (!fastRenameBinding(binding.scope, oldName, newName)) {
  binding.scope.rename(oldName, newName); // :446 — always renames
}
```

The module-binding apply does not:

```ts
fastRenameBinding(mb.scope, oldName, newName); // :987 — return ignored, no fallback
usedNames.delete(oldName);
usedNames.add(newName);
```

`fastRenameBinding` returns `false` for export-involved bindings (it
declines so the caller can use Babel's export-aware `scope.rename`). When
that happens for a module binding, the AST is **not** renamed, but
`applyValidRenames` still records `status: "renamed"` and `usedNames` is
updated as if it were. Result: the binding is reported renamed and its
new name marked "taken" while the code still shows the old name. Latent
today because the Bun-CJS target has no ESM exports (so
`fastRenameBinding` never declines for module bindings), but it is a real
divergence and will bite if ESM export bundles are processed. Fix: mirror
the function path — honor the return and fall back to `scope.rename`.
Better: route both through `attemptValidatedRename` (see B1).

### A3. `reconstructClusters` degrades the manifest — filename-stem ids, empty hashes

**`src/split/index.ts:303-310`. Medium-high confidence.**

The adapter boundary flattens the rich `Cluster[]` to
`Map<sessionId, filename>`, then `reconstructClusters` rebuilds `Cluster`
objects with `id: name.replace(/\.js$/,"").slice(0,16)`,
`rootFunctions: []`, `memberHashes: []`. So `generateManifest`
(index.ts:712-718) always emits empty `memberHashes`/`rootFunctions` and
an `id` that is a filename stem, not the sha256 content fingerprint the
`Cluster` type and clustering code (A/C4) advertise. Any consumer treating
manifest `cluster.id` as a stable content fingerprint is wrong. Likely a
side-effect of the unified-adapter refactor; the fingerprint isn't fully
dead (`nameCluster` uses it before flattening), so this is specifically a
final-plan/manifest degradation.

### A4. The cross-file import graph is built twice, independently

**`src/split/index.ts:432-489` (cycle resolution) vs `src/split/emitter.ts:347-407` (emission). Medium-high confidence.**

`groupLedgerEntries` + `addRefToImports` + `buildImportsForFile` build
`file → {file → names}` for `resolveImportCycles`, which then **mutates
`entry.outputFile`**. `groupEntriesByFile` + `addCrossFileRef` +
`buildFileImports` + `buildNameToFile` rebuild the same graph for actual
emission (one uses `Set`, the other array+`includes`; otherwise
equivalent today). The cycle resolver breaks cycles in graph #1; the
emitter emits from graph #2. If either grows a rule the other lacks
(builtins, re-exports, index handling), the resolver "fixes" cycles that
don't match what's emitted → circular imports in output. Unify into one
import-graph builder both consume.

### A5. Library banner scan diverges in scope between adapters

**`src/library-detection/adapters/default.ts:120-131` vs `adapters/bun.ts:121-130`. High confidence.**

Same "return the first banner library name" operation, two
implementations: `detectLibraryFromHeader` scans `code.slice(0, 1024)`
with `String.match`; `scanForBanner` scans the **entire file** with a
freshly-built `RegExp` + `.exec`. A banner past byte 1024 makes a file
"library" under the bun adapter but "app/mixed" under the default adapter
— inconsistent classification of the same input. Extract one
`firstBannerName(code, limit?)` shared with `comment-regions.ts`
`findCommentRegions` (a third copy of the same scan).

### A6. `pickBestFile` lacks the deterministic tiebreak its siblings have

**`src/split/index.ts:144-154` vs `call-graph.ts:168-176` and `cluster.ts:471-480`. High confidence divergence; determinism impact.**

`pickBestFile` argmaxes file-reference counts with **no tiebreak**
(first-inserted wins). Its siblings `pickBestClusterByCount` and
`findBestMergeTarget` add a lexicographic `id` tiebreak. `pickBestFile`
drives helper→file assignment (index.ts:264/273/582), so on a count tie
the output file is chosen by iteration order — non-deterministic across
runs, contrary to the subsystem's determinism convention. (Sibling
instance: `reference-cluster.ts:671` `findLargestCluster` vs
`call-graph.ts:182` `pickLargestCluster` — same divergence, currently
masked by pre-sorted input.)

### A7. Browserify's webpack-exclusion guard is a drifted subset of webpack's markers

**`src/detection/signals/browserify.ts:11` vs `signals/webpack.ts:4`. Medium confidence.**

Webpack is detected on three markers (`__webpack_require__`,
`__webpack_modules__`, `webpackChunk`). Browserify's "not webpack" guard
checks only `__webpack_require__`, then emits a **definitive** browserify
signal on `installedModules`. A webpack bundle whose scanned window has
`webpackChunk`/`__webpack_modules__` but not the literal
`__webpack_require__` (common in minified chunks where it's a param) can
emit a spurious definitive browserify signal. Masked today only because
webpack runs first in `BUNDLER_DETECTORS` and `detect.ts` takes `[0]`.
Guard should exclude on the same marker set webpack detects on (shared
constant).

### A8. `createRequire` import regex drifted between detector and extractor

**`src/detection/signals/bun.ts:16` vs `src/shared/bun-helpers.ts:71`. Medium confidence.**

Detector: `/import\s*\{[^}]*createRequire[^}]*\}.../` (loose substring,
alias optional). Extractor: `/...createRequire\s+as\s+(\w+).../` (requires
`as <alias>`). A bundle can pass the detector's bun gate yet have
`identifyBunRequire` return `null`, so `rewriteRequireCalls` silently
never runs. Graceful degradation, but the two regexes for one marker
should agree.

---

## B. Redundant / architecturally-divergent paths

### B1. Three rename-application paths with different validation

**`attemptValidatedRename` (validated-rename.ts) vs `applyFunctionRename` (processor.ts:416) vs module-binding `applyRename` (processor.ts:984). Medium confidence, maintainability risk.**

`attemptValidatedRename` is the sanctioned path: `getRenameRejection`
(six checks incl. `target-free-name`/`target-visible`/`shadows-child`) →
apply → post-rename spot check. The two LLM paths reimplement application
with weaker local checks and rely on validation being spread across other
layers (`validateBatchRenames` + the `wouldShadow` callback +
`fastRenameBinding`'s throw). They are safe **today** because `usedNames`
happens to include ancestor bindings and globals — but each new invariant
(e.g. review C1's `target-free-name`) has to be re-added in every path or
they silently drift. A2 is the concrete instance already drifted. Fix:
funnel all three through `attemptValidatedRename` (the "validated rename
single path" the memory already calls for), or at minimum share one
apply-and-record helper.

### B2. `assignOrphans` recomputes the score `findBestClusterForOrphan` already found

**`src/split/reference-cluster.ts:734-740`. High confidence, small cost.**

`findBestClusterForOrphan` computes the winning cluster's overlap score
internally, returns only the index, then `assignOrphans` calls
`computeOverlapScore` again on the winner. Have the finder return
`{index, score}`. (See C3 — the overlap scorer is itself triplicated.)

### B3. [FIXED · b8680de] Provider-side `sanitizeIdentifier` makes the entire "invalid → retry" subsystem dead code

**`src/llm/openai-compatible.ts:92` & `:241` vs `src/rename/processor.ts:2353`,`:2391`,`:2571`. Medium-high confidence; behavioral consequence.**

The provider sanitizes every LLM-returned name before it leaves the
adapter (`renames[oldName] = sanitizeIdentifier(newName)`). The processor
then re-sanitizes (`processor.ts:2353`) and tries to reject invalids
(`if (!isValidRenameTarget(newName)) invalid.push(...)`, `:2391`, and the
twin at `:2571`). But `sanitizeIdentifier` (`validation.ts:131-151`)
_always_ returns a legal, non-reserved, non-builtin identifier — it
appends `_` for reserved/builtin — so `isValidRenameTarget(sanitizeIdentifier(x))`
is **always true**. Every production name flows through the provider, so:

- the `invalid` branch never fires;
- the "invalid" retry-feedback scaffolding is unreachable — the
  `"...not allowed (reserved word, global built-in, or invalid syntax)"`
  prompt text duplicated in **both** `prompts.ts:144` and `:420`, the
  `invalid` diagnostics bucket, etc.;
- a reserved-word/builtin suggestion (`delete`, `Map`) is silently
  rewritten to `delete_`/`Map_` and applied, instead of being surfaced to
  the model for a better name as the retry loop intends.

This is the largest chunk of dead/redundant logic in the reviewed set. Fix
one way or the other: stop sanitizing in the provider (let the processor
classify raw names `invalid` and retry), **or** delete the unreachable
invalid-handling + its prompt scaffolding. (Not wrong output — `Map_` is
valid — but a whole feedback subsystem that can never run.)

---

## C. Pure duplication (house rule: "actively unify duplicated code")

No active bug, but each is a drift hazard — the exact shape where a change
lands in N-1 of N copies.

- **C1. Cluster fingerprint recipe ×4** — identical
  `sha256(sortedMemberHashes.join(",")).slice(0,16)` at `cluster.ts:127`,
  `:344`, `:711`, `reference-cluster.ts:573`. `:344` and `:711` are
  byte-identical. Extract `computeClusterFingerprint`.
- **C2. Adapter `groupFunctions` ×3** — `esbuild-esm.ts:22`,
  `esbuild-cjs.ts:23`, `bun-cjs.ts:22` are identical ("assign by module
  position, unassigned → shared.js"). One shared helper / base method.
- **C3. IDF-overlap scoring ×3** — `reference-cluster.ts:605` (inlined),
  `:697` `computeOverlapScore`, `:970` `computeRefOverlap` (differ only in
  undefined-handling). Drift risk: a switch to Jaccard in one leaves the
  other similarity measure stale.
- **C4. `escapeRegExp` ×3, identical** — `shared/bun-helpers.ts:111`,
  `unpack/adapters/bun.ts:366`, `split/module-detect.ts:240`. Export one.
- **C5. Bun `{exports:{}}` factory regex ×2** — `detection/signals/bun.ts:12`
  and `shared/bun-helpers.ts:29` (identical today; detector + extractor for
  one shape — same class as A8). Plus the "no factory → write index.js"
  fallback appears 3× (`unpack/adapters/passthrough.ts:16`,
  `bun.ts:62`, `:78`).
- **C6. `placeholderMapping ?? buildPlaceholderMapping(fn.path)` ×3** —
  `prior-version.ts:466`, `:557`, `:559`. Extract `resolvePlaceholderMapping(fn)`.
- **C7. Backwards-binding-walk regex ×2** — identical
  `/(?:(?:var|let|const)\s+|,)([$\w]+)\s*=\s*[^;]*$/` in
  `bun-helpers.ts:45` and `:103`.
- **C8. `sessionId → node.id.name` builders** — `call-graph.ts:56`
  `buildFunctionNameMap` and `reference-cluster.ts:753` `buildFunctionNames`
  are identical; the `"id" in node && node.id && node.id.name` idiom is
  copied ~6× across split/. One shared extractor.
- **C9. Retry-diagnostics renderer duplicated + drifted** —
  `renderRetryDiagnostics` (`prompts.ts:119`, function path) and
  `buildModuleLevelRetryPrefix` (`prompts.ts:397`, module path) are the same
  "your previous suggestions had issues" block, but the module copy dropped
  the `else` branches (`prompts.ts:130-137`/`141-148` have them; `:408-413`/
  `:417-422` don't). A `duplicate`/`invalid` identifier with no recorded
  prior suggestion gets a feedback line in the function retry but **none** in
  the module retry. Low runtime impact (a real duplicate usually has a
  `lastSuggestion`), but it's precisely the "two copies, one missing a rule"
  drift — collapse to one shared renderer.
- **C10. `formatDuration` re-implemented in coverage.ts** — exported from
  `metrics.ts:364` and already reused by `profiling/summary.ts` and
  `ui/progress.ts`, but `coverage.ts:365` `fmtDuration` is a byte-identical
  private copy. `coverage.ts` already imports from `../llm/metrics.js` —
  just use the export.
- **C11. Response-format tail helper exists but is inlined at 2 of 3 sites** —
  `buildRenameResponseInstruction` (`prompts.ts:111`) is the shared
  `{ "id": "descriptiveName", … }` renderer, yet `buildBatchRenamePrompt`
  (`prompts.ts:89`) and `buildModuleLevelRenamePrompt` (`:387`) hand-roll the
  same `identifiers.map(...).join(", ")` instead of calling it.

---

## Cross-check: things that looked divergent but are NOT (verified safe)

- **Two Bun-factory detectors** (`identifyBunCjsFactory` source-regex vs
  `classifyBunModules` AST walk): NOT divergent — `classifyBunModules`
  seeds its AST walk from `identifyBunCjsFactory`'s result, so they cannot
  disagree.
- **Nullable module-binding `fingerprint`** (from the 5.4 change): every
  `.fingerprint.structuralHash` read outside analysis is on a `FunctionNode`
  (non-null); the two `ModuleBindingNode` reads are null-guarded
  (`fingerprint-index.ts:62`, `prior-version.ts:702`). Safe.
- **`checkNodeReady` vs `checkNodeReadyIgnoringScopeParent`**: deliberate
  Tier-1 deadlock-break variant, not accidental drift.
- **LLM apply path vs `getRenameRejection` (the B1 divergence)**: independently
  confirmed safe by a second reviewer — the main LLM path skips
  `getRenameRejection`, but `getUsedNames()` merges `context.usedIdentifiers`
  (which walks the full ancestor scope chain **plus** program globals via
  `context-builder.ts:119-136`) with the module set, so `target-visible` and
  `target-free-name` capture are covered mechanically and child-shadow by
  `wouldShadow`. Factored differently, same coverage. B1 is a maintainability
  risk, not an active bug.
- **Metrics / coverage counting**: `strategyKey` (`coverage.ts:51`) is
  exhaustive against `RenameReport["strategy"]`; both `renamedCount`
  construction sites agree with their `outcomes`. No double-count or drift.
- **Identifier-profile prompt formatting**: the batch prompt (function code +
  callee/callsite) and the module prompt (per-identifier mini-profiles) are
  genuinely different formats, not duplicated logic.

---

## 119 → 120 full-bundle validation run — plan

**Goal (per request):** see what the actual cross-version diff looks like
after humanification, at current `main` (post item-5 refactor), and confirm
the refactor didn't regress the full-bundle path.

### Why re-run now

The last full run (`PHASE4-RESULTS.md`) was pinned at `1c83a83` — **before**
the entire item-5 structural refactor (collector unification, prior-transfer
extraction, shim deletion, RunConfig, nullable fingerprints, lifecycle state
machine) and before the `internalErrors` counter (`d0db823`) that replaces
Phase 4's derived "Failed: 2/8" accounting residual. The 33 fingerprint
e2e snapshots are unchanged across the refactor, so the expectation is
**near-identical numbers** to Phase 4 — the run's job is to confirm that at
full-bundle scale and to produce a fresh diff to characterize.

### Command

The harness is already parameterized. From repo root, with the local LLM box
up (`http://192.168.1.234:8000/v1`, `gpt-oss-20b`):

```bash
git worktree add /tmp/humanify-run-120 HEAD          # pin the run to current main
ln -s "$PWD/node_modules" /tmp/humanify-run-120/node_modules
cd /tmp/humanify-run-120
PHASE2_OUT=/tmp/exp013-phase5 bash experiments/013-bun-cjs-classification/run-phase2.sh
```

`run-phase2.sh` runs: **Run A** = humanify v119 fresh (`--bundler bun
--minifier bun --reasoning-effort low`), **Run B** = humanify v120 with
`--prior-version <Run A runtime.js>`, then parse-validates both and emits
`diff` line/hunk stats. Inputs:
`claude-code-2.1.{119,120}/binary-decompiled/src/entrypoints/index.js`.

### Baselines to diff against (Phase 4, commit 1c83a83)

| metric                          | Phase 4                   | expectation at HEAD                        |
| ------------------------------- | ------------------------- | ------------------------------------------ |
| incremental wall clock          | 8m56s                     | ≈ same (no perf change since)              |
| functions exact-transferred     | 34,057                    | ≈ same (shorthand fix already in)          |
| functions close-matched         | 7,488                     | ≈ same                                     |
| functions pure-fresh LLM        | 1,094                     | ≈ same                                     |
| module bindings cached          | 84.3% (14,637)            | ≈ same (single-vote recovery NOT done)     |
| diff lines / rename-noise hunks | 138,968 / 21,436          | ≈ same                                     |
| correctness                     | 0 parse / 0 semantic fail | **0 required**; `internalErrors` must be 0 |

### What to actually look at in the diff (the point of the run)

1. **`internalErrors`** in the coverage summary must be 0 — this is the
   first full run with the real counter (item 4e); any nonzero value is a
   programming error surfaced by the refactor and blocks.
2. **Classify `runtime-diff.txt` hunks** into three buckets to understand
   the residual: (a) genuine v119→v120 source changes, (b) rename-noise
   (equal-count change hunks identical after
   `\b[A-Za-z_$][A-Za-z0-9_$]*\b` → `#`), (c) library-body byte deltas from
   Bun re-rolling minified names. Buckets (b)+(c) are the reducible noise.
3. **Attribute the rename-noise per population** via the diag JSONs
   (`cc-120-diag.json`): is the ~21K-hunk noise still dominated by the
   ~980 dropped module-binding transfers (the designed precision trade),
   or has the population shifted? This tells us whether the top follow-up
   lever — recover safe single-vote binding transfers (exact-matched voter
   - prior-unique name) — is still the biggest win.
4. **Sanity-check the lifecycle refactor at scale**: matcher stats
   `singletonRejected`/`injectivityDemoted` nonzero is expected/fine; watch
   for any graph-closure assertion or Tier-2 force-break (Phase 4 had
   none).

### Follow-up levers this run informs (from the handoff, not yet done)

- Recover safe single-vote binding transfers (biggest noise block, ~980).
- Per-population noise attribution via diag JSONs.
- Operator normalization (the biggest remaining naming-consistency win per
  the lost cross-version-diff-gaps note).
