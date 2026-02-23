# Spec 19: Deterministic File Splitting

## Problem

After humanify deobfuscates a JavaScript bundle, the output is flat files — either a single large file (Rollup/esbuild/Bun scope-hoisted bundles) or multiple flat files (webpack/browserify). We want to reconstruct a directory/file structure resembling a normal codebase.

**Critical requirement:** Given runs on v1.0 and v1.1 of the same codebase (where most code is identical), the file split must produce the same structure so that `diff -r v1.0-output/ v1.1-output/` shows only meaningful changes.

>> the goal here is that we will have a stable output for a given input, and its stability is relative to the amount of code changed. 

## Design Constraints

- **Bundler-agnostic:** Works on any minified JS. Can opportunistically use webcrack metadata (module paths, bundle type) but doesn't require it.
- **Discrete step:** Runs after the LLM rename pass. Can be invoked standalone (`humanify split output/`) or as part of the full pipeline (`humanify unminify --split`).
- **LLM-suggested names:** File and directory names are semantically meaningful, cached by cluster fingerprint for cross-version stability.
- **Incremental delivery:** Layer 1 (library separation) ships first, then app code splitting.

## Available Signals

| Signal | Source | Stability | Scope |
|--------|--------|-----------|-------|
| `modulePath` | webcrack metadata | Perfect | Webpack/browserify only |
| `FunctionNode.fingerprint.exactHash` | Normalized AST hash | Excellent (minification-invariant) | All bundlers |
| `internalCallees` / `callers` | Call graph | Good (structural) | Per-file |
| `scopeParent` | Scope nesting | Good | Per-file |
| `CommentRegion` | Library banners | Good | Rollup/esbuild |
| `externalCallees` | API call names | Medium | All |
| Position adjacency | AST order | Poor (bundler may reorder) | All |

>> lets talk about fingerprint.exactHash as that is pretty key to a lot of this working well.  We wikl want some extensive tests on the hashing behavior to understand how it works

## Architecture

### Pipeline Integration
>> we should probably make sure the general project supprots being used as a pipeline, so we have distinct stages and determine which we execute, and what plugs in to each stage. it will help with things like handling different compaction/minification tools etc
```
# Discrete steps:
humanify unminify bundle.js -o output/        # Steps 1+2: unpack + rename
humanify split output/ -o structured/          # Step 3: file splitting

# Combined:
humanify unminify bundle.js -o output/ --split
```

### Metadata Sidecar

The rename pass emits a sidecar file so the split step can work without re-parsing:

>> in another spec file we started speccing out a sqlite db for saving function hashes etc, we might want to consider if having a more consistent state backend would be helpful, though we would want a way to serialize the output to something more readable

**`output/.humanify-meta.json`:**
```json
{
  "version": 1,
  "bundleType": "webpack" | "browserify" | null,
  "files": {
    "app.js": {
      "libraryRegions": [
        { "libraryName": "react", "startLine": 1, "endLine": 450 }
      ],
      "functions": [
        {
          "fingerprint": "a3f2b1c4d5e6f7a8",
          "name": "createAuthProvider",
          "startLine": 452,
          "endLine": 510,
          "callees": ["b2c3d4e5f6a7b8c9"],
          "callers": [],
          "scopeParent": null
        }
      ]
    }
  }
}
```

## Layer 1: Library Separation (Milestone 1)

Move detected library code into `vendor/{libraryName}/` subdirectories.

### Input Signals

- `WebcrackFile.metadata.modulePath` containing `node_modules/` (webpack/browserify)
- `CommentRegion` data persisted in sidecar (all bundlers)
- Library banner comments still present in deobfuscated output

### Algorithm

1. Read `.humanify-meta.json` sidecar (or re-detect library regions if running standalone on non-humanify output)
2. **Webpack/browserify:** Organize by recovered module path → `vendor/react/index.js`, `vendor/lodash/merge.js`, etc.
3. **Rollup/esbuild:** Extract comment-delimited library regions into `vendor/{libraryName}.js`
4. Remaining app code → `src/` directory

### Output Structure

```
structured/
  vendor/
    react.js
    lodash/
      merge.js
      cloneDeep.js
  src/
    app.js              # remaining app code (unsplit until Milestone 2)
```

### Edge Cases

- Library region spans partial functions → include the whole function in vendor if >50% of its body is in the library region

>> I dont quite understand what this means. That somehow we would have a library funciton inside an app function?

- Multiple libraries interleaved (Rollup) → each gets its own file under vendor/
- No library detection data available → skip Layer 1, pass everything to Layer 2

## Layer 2: App Code Clustering (Milestone 2)

Split remaining app code into logical modules using fingerprint-stable dependency trees.

### Algorithm: Fingerprint-Stable Dependency Trees

**Step 1: Build function graph**

Parse the deobfuscated app code and build the function graph using `buildFunctionGraph()`. This gives us `FunctionNode` objects with `internalCallees`, `callers`, `fingerprint`, and `scopeParent`.

>> we will want to make sure that we can build this if we are running standalone, but that we can also receive the already parsed graph from the renaming step if we are doing it all at once, so it can use the same sturcutre and format

**Step 2: Identify root functions**

Root functions have `callers.size === 0` — nothing else in the file calls them. These are effectively the module's public API / exports.

**Step 3: Grow clusters by exclusive ownership**

```
for each root R:
  cluster[R] = { R }
  queue = [...R.internalCallees]
  while queue is not empty:
    fn = queue.pop()
    if fn is reachable from exactly one root:
      cluster[R].add(fn)
      queue.push(...fn.internalCallees)

shared_cluster = functions reachable from multiple roots
```

>>> we should come up with a few test cases and end to end examples we can use to understand this behavior and test it on some larger projects.  Similar to our other end to end test cases, we can start with a open source repo, minify it outself, unminify it and split, and then compare the layouts

**Step 4: Cluster non-function declarations**

Top-level variables, classes, and constants are assigned to clusters using Babel scope analysis:
- Referenced by one cluster → belongs to that cluster
- Referenced by multiple clusters → `_shared.js`   
>> we may want a better name than this, but the idea is correct

- `const` declarations can be duplicated if small (< 100 chars) to reduce cross-file imports

**Step 5: Compute cluster identity**

```typescript
function clusterFingerprint(members: FunctionNode[]): string {
  const hashes = members.map(m => m.fingerprint.exactHash).sort();
  return sha256(hashes.join(':')).slice(0, 16);
}
```

This is deterministic and stable: same member functions → same cluster fingerprint.

### Cross-Version Stability

- Same functions (by `exactHash`) → same cluster → same file

>> what is exact hash referencing here? 
- New function added, called only by existing root → joins that cluster. Cluster fingerprint changes but **root identity** (root function's own fingerprint) is stable → file keeps its name.
- Function moves to a different caller in v1.1 → cluster membership changes → genuine structural change → shows up as a file move in diff.
- Cluster matching across versions: match by highest fingerprint overlap (>70% member fingerprints shared → same logical cluster).

>> lets make some explicit diagrams/examples for how we expect this to work

### Determinism Guarantees

- Sort by `exactHash` at every decision point
- Break ties with secondary sort on serialized `StructuralFeatures`
- Never depend on `Map`/`Set` iteration order for output-affecting decisions
- Use stable sort throughout (Node.js `Array.sort` is stable in V8)

## Layer 3: Import/Export Reconstruction

After clustering, reconstruct ES module boundaries:

### Exports

For each cluster, identify functions/declarations used by other clusters:
```typescript
for each function F in cluster C:
  for each caller of F:
    if caller is in a different cluster:
      mark F as exported from C
```

>> is there a network/graph library we want to use to speed up finding bridges between clusters?

Emit as named exports:
```javascript
export function createAuthProvider(...) { ... }
export const AUTH_CONFIG = { ... };
```

>>> are we able to do these deterministically using babel/another tool? I want to avoid asking an LLM to just "write" a file but instead want the transition to be a series of operations that dont rely on an LLM to not drop a character or change something (similar to how our renaming works)

### Imports

For each cluster, identify functions/declarations it uses from other clusters:
```javascript
import { createAuthProvider } from './auth-provider.js';
import { AUTH_CONFIG } from './_shared.js';
```

### Shared Mutable State

Mutable (`let`/`var`) declarations referenced by multiple clusters go to `_shared.js`:
```javascript
// _shared.js
export let currentUser = null;
export function setCurrentUser(user) { currentUser = user; }
export function getCurrentUser() { return currentUser; }
```

>> for all of these we want to understand how we can log the steps and changes for -v or -vv debugging as well as having a --dry-run option or similar so that we can visualize what the clusters would look like / changes that would be applied

**Simplification for v1:** Only split functions and `const` declarations. All mutable shared state stays in `_shared.js`. This handles ~80% of cases.

## Layer 4: LLM-Suggested File Naming

### Algorithm

1. For each cluster, build a context summary:
   - Root function name(s)
   - All exported function names
   - External API calls (`externalCallees`)
   - String literals from structural features
2. Send to LLM: *"Given these functions and their relationships, suggest a file path like `src/auth/provider.js` or `src/utils/formatting.js`"*
3. LLM responds with suggested path
4. Cache: `{ clusterFingerprint → suggestedPath }` in `split-names.json`

### Cross-Version Name Stability

**`split-names.json`** persists across runs:
```json
{
  "version": 1,
  "names": {
    "a3f2b1c4d5e6f7a8": "src/auth/provider.js",
    "b2c3d4e5f6a7b8c9": "src/utils/formatting.js"
  }
}
```

Lookup strategy:
1. Exact match: cluster fingerprint found in cache → use cached name
2. Fuzzy match: >70% member fingerprints overlap with a cached cluster → inherit cached name
3. No match: ask LLM for a new name, cache it

This ensures the LLM is only consulted for genuinely new clusters. Existing clusters keep their names even if the LLM would suggest something different on a fresh run.

>>> our algorithm here is eseentially an experiment, we should treat its development as such. Lets build a test set of several smaller libraries/fixtures that we can use and validate against and track the performance of the system as we iterate on the algorithm

### Fallback Naming (no LLM available)

If running without LLM access (e.g., `humanify split --no-llm`):
- Use root function's humanified name: `createAuthProvider.js`
- If no clear root: truncated cluster fingerprint: `mod_a3f2b1c4.js`

## New Files

| File | Purpose |
|------|---------|
| `src/split/index.ts` | Main orchestrator, CLI integration |
| `src/split/types.ts` | Cluster, SplitPlan, SplitResult types |
| `src/split/metadata.ts` | Read/write `.humanify-meta.json` sidecar |
| `src/split/library-separator.ts` | Layer 1: extract library code to vendor/ |
| `src/split/cluster.ts` | Layer 2: fingerprint-stable dependency tree clustering |
| `src/split/import-export.ts` | Layer 3: reconstruct imports/exports |
| `src/split/naming.ts` | Layer 4: LLM-suggested file names with caching |

## Modified Files

| File | Change |
|------|--------|
| `src/unminify.ts` | Emit metadata sidecar; optionally invoke split |
| `src/plugins/rename.ts` | Export function graph data for sidecar |
| `src/index.ts` | Add `split` subcommand and `--split` flag |
| `src/analysis/types.ts` | Extend FunctionNode with optional cluster assignment |

## Existing Code to Reuse

| Module | Usage |
|--------|-------|
| `src/analysis/function-graph.ts` → `buildFunctionGraph()` | Call graph for clustering |
| `src/analysis/structural-hash.ts` → `computeExactHash()` | Cluster fingerprint computation |
| `src/analysis/function-fingerprint.ts` | Cross-version cluster matching |
| `src/library-detection/` (all layers) | Layer 1 library separation |
| `src/rename/processor.ts` | Pattern for LLM batching (reuse for naming) |

## Verification

### Milestone 1 (Library Separation):
- Run on a webpack bundle → `vendor/` contains library files organized by module path
- Run on a Rollup bundle → library code extracted by banner detection to `vendor/{name}.js`
- Run twice on same input → identical output (determinism check)
- Run standalone `humanify split` on pre-existing output → works without re-running rename

### Milestone 2 (App Code Splitting):
- Run on Rollup single-file output → app code split into multiple files under `src/`
- Run on v1.0 and v1.1 of same app → same file structure; diff shows only changed functions
- Reconstructed imports/exports produce valid JS (`node --check` on each output file)
- Cluster fingerprints stable across runs (log and compare)
- LLM naming cached in `split-names.json` and reused on repeat runs

## Open Questions

1. Should the sidecar metadata include the full AST or just the function graph summary? Full AST avoids re-parsing but is large.
>> probably the full ast, though we can also figure out it can be stored in sqlite, so we can use that as the backing store to allow run resumption

2. For the "exclusive ownership" clustering — what threshold should trigger splitting a large cluster? (e.g., if one root has 200 functions, should we sub-cluster?)
>> good question, maybe we can take some popular open source libraries/ libraries we thing are well organized and analyze their layouts

3. Should `_shared.js` be further split if it grows large (e.g., by grouping shared utilities by their callers)?
>> yes, i think all of our splitting will be recursive or cutoff at certain thresholds. Our target with this program is to re-organize the uniminified claude code, which is 40k+ functions
4. How should we handle circular call relationships between roots? (Both are roots, both call each other — they probably belong in the same cluster.)

>>> how do normal js libraries handle this cross calling problem?
5. For the LLM naming pass — should we also suggest directory structure (e.g., `src/auth/` vs `src/utils/`) or just flat file names?

>> hmm, given that we probably want to split up large clusters, we may want to have each cluster get a named based on its functions, that we use for a dir name, and then recruse to split out sub folders or file names depending on size

>> 6.  How are we going to perform these operations, we want to be mechanical (aka we have a moveFunction(inputfile, outputfile) or similar) so we dont rely on an llm to actually rewrite the data but instead just suggest the names / operations to be done.  This is similar to how we handle LLM function renaming, where the llm suggests the names, but the actual renaming is done via babel ast rename
