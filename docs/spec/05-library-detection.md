# Library Detection

## Overview

Minified bundles often contain library code (React, lodash, etc.) alongside application code. Processing library code is wasteful:

1. **No benefit** — Library code is already well-documented; users can reference official docs
2. **Cost** — Library code can be 80%+ of a bundle; processing it wastes LLM calls
3. **Worse results** — LLM might rename `useState` to something confusing

## Goals

1. Identify known libraries in unpacked bundles
2. Mark library files and functions as "external" (skip processing)
3. Focus LLM resources on novel application code

## Current Implementation (Layer 1 & 2)

The initial implementation lives in `src/library-detection/` and operates at the **file level** in `src/unminify.ts`. It runs after webcrack extracts modules but before the plugin loop (babel → rename → prettier).

### Layer 1: Module Path Matching

After webcrack unpacks, the `WebcrackOutput` includes `ModuleMetadata` per file (module ID, resolved path, isEntry flag). We check module paths against known library patterns:

```
src/library-detection/detector.ts → isLibraryPath()
```

Patterns matched:
- `node_modules/` anywhere in the path
- `@babel/runtime`, `core-js`, `regenerator-runtime`, `tslib`, `webpack/runtime`

This is the fastest and most reliable signal. Webpack bundles preserve module paths in their module map, and webcrack recovers them.

### Layer 2: Comment/Banner Detection

```
src/library-detection/comment-patterns.ts → detectLibraryFromComments()
```

Scans the first 1KB of each file for preserved copyright/license banners:
- `/*! library-name v1.2.3 */` — common in webpack production builds
- `/** @license library-name */` — JSDoc license tags
- `/** @module library-name */` — JSDoc module tags
- `* library-name v1.2.3` — version strings inside block comments

Only fires if Layer 1 doesn't match (no file read needed when path matches).

### Integration Point

In `unminify.ts`, after `webcrack()` returns files:

```typescript
const { files, bundleType } = await webcrack(bundledCode, outputDir);

if (skipLibraries) {
  const detection = await detectLibraries(files);
  filesToProcess = files.filter(f => !detection.libraryFiles.has(f.path));
}
```

Detected library files are logged and excluded from the processing loop. The `--no-skip-libraries` CLI flag disables this.

## The Rollup Problem

Layers 1 and 2 work well for **webpack** and **browserify** bundles because those bundlers wrap each module in its own function, and webcrack extracts them as separate files with preserved module paths.

**Rollup and esbuild are different.** They use "scope hoisting" — all modules are concatenated into a single scope (or a few chunks) with renamed top-level bindings to avoid conflicts. After webcrack processes a Rollup bundle:

1. **No module boundaries** — webcrack may output a single file (or very few files) because there are no wrapper functions to detect
2. **No module paths** — there's no webpack module map to recover paths from
3. **Mixed code** — library functions (React internals, lodash helpers) sit directly alongside application functions in the same file, interleaved in the same scope
4. **Comment banners may survive** — but they mark a region of the file, not the whole file

This means Layer 1 (path matching) produces nothing, and Layer 2 (comment scanning the first 1KB) might catch one banner but miss libraries deeper in the file.

### What Rollup Output Looks Like

A typical Rollup/esbuild bundle after minification:

```javascript
/*! React v18.2.0 */
var La=Symbol.for("react.element"),Ma=Symbol.for("react.fragment");
function Na(e,t,n){var r={$$typeof:La,type:e,key:n,ref:null,props:t};return r}
// ... hundreds of React functions ...

/*! zustand v4.5.0 */
function Oa(e){var t=typeof e;return e!=null&&(t=="object"||t=="function")}
// ... zustand functions ...

// application code — no banner
function Pa(e){return Na("div",{className:"app",children:[Qa(e.user)]})}
function Qa(e){return Na("span",null,e.name)}
```

Everything is in one file. The React and zustand functions are interleaved with application code. After webcrack deobfuscates/unminifies, the banner comments may or may not survive, and the code is still in one file.

## Layer 3: Intra-file Comment Region Detection

To handle Rollup bundles, we need to scan the **entire file** for banner comments (not just the first 1KB) and use them to mark **regions** of the file as library code.

### How It Works

1. Scan the full file for all banner comments (same patterns as Layer 2, but `matchAll` across the whole file)
2. Each banner marks the start of a library region
3. A library region extends from the banner to either the next banner or the next function that doesn't match the library's structural characteristics
4. Map each `FunctionNode` to a region based on its source position

```typescript
interface CommentRegion {
  libraryName: string;
  startOffset: number;       // byte offset of the comment
  endOffset: number | null;  // byte offset of next region, or null for end-of-file
}

function findCommentRegions(code: string): CommentRegion[] {
  // matchAll across full file for banner patterns
  // Sort by offset
  // Each region extends to the start of the next region
}
```

### Region Boundary Heuristic

The naive approach (region extends to next banner) overclaims: if React code ends at line 500 and zustand starts at line 800, lines 500-800 might be application code with no banner. To avoid this, we can use a **gap heuristic**:

- If there's a large gap (>100 lines) between the last function that looks like it belongs to the library and the next banner, end the region at the gap
- "Looks like it belongs" = no `export` statements (Rollup strips library exports), single-letter variable names, no domain-specific strings

This is deliberately conservative. Regions that are ambiguous should be classified as novel (processed by the LLM) rather than skipped. **False negatives (processing a library function) cost LLM calls. False positives (skipping application code) lose user work.**

### Integration with Function Graph

Layer 3 produces a set of `FunctionNode` sessionIds to skip. This hooks into the rename processor, not the file-level loop in unminify.ts.

The key integration point is `RenameProcessor.processAll()` in `src/rename/processor.ts`. Currently it receives all functions and processes them leaf-first. With library detection:

```typescript
// In src/plugins/rename.ts, createRenamePlugin():
const functions = buildFunctionGraph(ast, "input.js");

// NEW: classify functions before processing
const libraryFunctionIds = detectLibraryFunctions(ast, code, functions);

// Filter before handing to processor
const novelFunctions = functions.filter(fn => !libraryFunctionIds.has(fn.sessionId));

// Mark library functions as done so callers don't wait for them
for (const fn of functions) {
  if (libraryFunctionIds.has(fn.sessionId)) {
    fn.status = "done";
  }
}

const processor = new RenameProcessor(ast);
await processor.processAll(novelFunctions, provider, { concurrency, metrics });
```

Library functions are marked `status: "done"` so that any novel function that calls a library function doesn't get stuck waiting for it in the ready queue. The library function's code is left as-is (minified names preserved), which is correct — we don't want to rename React internals.

## Layer 4: Structural Fingerprint Matching (Future)

For Rollup bundles where banners have been stripped, we need a content-based approach. This uses our existing fingerprinting infrastructure.

### Reference Fingerprint Database

Pre-compute fingerprints for popular libraries by running `buildFunctionGraph` + `computeFingerprint` against their published source:

```typescript
interface LibraryFingerprintDB {
  version: number;
  libraries: LibraryEntry[];
}

interface LibraryEntry {
  name: string;
  npmPackage: string;
  versions: VersionEntry[];
}

interface VersionEntry {
  version: string;
  /** exactHash values for all functions in this version */
  functionHashes: Set<string>;
}
```

To detect libraries in a bundle:
1. Build function graph for the file
2. For each function, look up its `exactHash` in the reference database
3. If a function matches a known library function, classify it as library code
4. If >N functions from the same library match, classify the whole cluster

### Why This Works

Our `computeStructuralHash` already normalizes identifiers to positional placeholders. This means a minified React function produces the same hash as the unminified source, as long as the AST structure is preserved. Minifiers change names but preserve structure — that's exactly what our hash is invariant to.

### Why This Might Not Work

- **Tree shaking**: Dead-code-eliminated functions won't be in the bundle, so missing hashes don't indicate absence
- **Babel transforms**: If the library was compiled through a different Babel config than we used for reference fingerprinting, the AST structure can differ (arrow→function, optional chaining→ternary, etc.)
- **Version drift**: Minor versions may add/change functions, requiring reference DB updates
- **Collision risk**: Different libraries might have structurally identical utility functions (identity, noop, etc.)

### Mitigation

- Require a **minimum cluster size** (e.g., ≥3 functions from the same library) to classify
- Use `StructuralFeatures` (string literals, property accesses, external calls) as secondary disambiguation when hashes alone are ambiguous
- Publish the reference DB as a separate npm package that can be updated independently
- Start with the top 20 libraries (React, React-DOM, Vue, Angular, lodash, moment, axios, redux, zustand, etc.) which cover the vast majority of bundle weight

## Detection Layers Summary

| Layer | Scope | Signal | Bundler Coverage | False Positive Risk |
|-------|-------|--------|------------------|---------------------|
| 1. Path matching | File | `node_modules/` in webcrack module path | Webpack, Browserify | Near zero |
| 2. Header comment | File | Banner in first 1KB | All (if banner preserved) | Very low |
| 3. Comment regions | Function | Banners throughout file → region mapping | Rollup, esbuild | Low (conservative regions) |
| 4. Fingerprint DB | Function | Structural hash match against reference | All | Low (cluster threshold) |

Each layer is strictly additive. They run in order; a file/function classified by an earlier layer is not re-examined by later layers.

## Types

```typescript
// src/library-detection/types.ts

interface LibraryDetection {
  isLibrary: boolean;
  libraryName?: string;
  detectedBy?: "path" | "comment" | "comment-region" | "fingerprint";
  moduleMetadata?: ModuleMetadata;
}

interface DetectionResult {
  /** Files classified as entirely library code (skipped in unminify loop) */
  libraryFiles: Map<string, LibraryDetection>;
  /** Files classified as application code (processed normally) */
  novelFiles: string[];
  /** Files with mixed content (processed, but individual functions may be skipped) */
  mixedFiles: Map<string, MixedFileDetection>;
}

interface MixedFileDetection {
  /** Function sessionIds within this file that are library code */
  libraryFunctionIds: Set<string>;
  /** Per-function detection details */
  functionDetections: Map<string, LibraryDetection>;
}
```

## CLI

```bash
# Auto-detect and skip libraries (default)
humanify bundle.min.js -o output/

# Force processing of everything (including libraries)
humanify bundle.min.js --no-skip-libraries -o output/

# Show what would be skipped (future)
humanify bundle.min.js --dry-run -v
# Output:
# Detected libraries:
#   react (1,247 functions, detected by: path)
#   lodash (412 functions, detected by: comment-region)
# Novel code: 89 functions
# Would process: 89 functions (skipping 1,659 library functions)
```

## Implementation Order

### Phase 1 (Done)
- [x] `WebcrackOutput` with module metadata
- [x] Layer 1: path matching (`isLibraryPath`)
- [x] Layer 2: header comment detection (`detectLibraryFromComments`)
- [x] File-level filtering in `unminify.ts`
- [x] `--no-skip-libraries` CLI flag
- [x] Unit tests for path matching and comment detection

### Phase 2: Mixed File Support
- [ ] Extend `DetectionResult` with `mixedFiles`
- [ ] Layer 3: full-file comment region scanning
- [ ] Map `FunctionNode` sessionIds to comment regions via source position
- [ ] Hook into `createRenamePlugin` to mark library functions as `done` before processing
- [ ] Integration test: Rollup bundle with React + application code
- [ ] Log mixed-file detection: "Skipping 412 library functions in chunk-abc123.js (react, lodash)"

### Phase 3: Fingerprint Database
- [ ] Build reference fingerprint tool: `npm run build-lib-fingerprints -- react@18 lodash@4`
- [ ] JSON database format and loader
- [ ] Layer 4: hash lookup during function graph construction
- [ ] Cluster threshold logic (≥3 matches from same library)
- [ ] Ship initial DB for top 20 libraries
- [ ] Benchmark against known bundles (see spec 15)

### Phase 4: Refinements
- [ ] `--dry-run` mode showing detection summary without processing
- [ ] Verbose logging of per-function detection decisions at `-vv`
- [ ] Handle edge case: library function that calls application code (don't skip the call target)
- [ ] Consider DEBUN-style property-order features as a Layer 4 enhancement (see spec 15)
