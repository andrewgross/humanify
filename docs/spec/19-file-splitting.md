# Spec 19: Deterministic File Splitting

## Problem

After humanify deobfuscates a JavaScript bundle, the output is flat files — either a single large file (Rollup/esbuild/Bun scope-hoisted bundles) or multiple flat files (webpack/browserify). We want to reconstruct a directory/file structure resembling a normal codebase.

**Critical requirement:** The output is stable for a given input, and its stability is proportional to the amount of code changed between versions. If v1.0 → v1.1 changes 5% of functions, roughly 5% of the output structure should differ. `diff -r v1.0-output/ v1.1-output/` should show only the meaningful changes.

## Design Constraints

- **Bundler-agnostic:** Works on any minified JS. Can opportunistically use webcrack metadata (module paths, bundle type) but doesn't require it.
- **Discrete pipeline step:** Runs after the LLM rename pass. Can be invoked standalone (`humanify split output/`) or as part of the full pipeline (`humanify unminify --split`). The overall project should support composable stages: unpack → rename → split, where each stage has well-defined inputs/outputs.
- **LLM-suggested names:** File and directory names are semantically meaningful, cached by cluster fingerprint for cross-version stability.
- **Mechanical transforms only:** The LLM suggests names and groupings. All actual code manipulation (extracting functions, adding imports/exports) is done via deterministic Babel AST operations — identical to how renaming works today (LLM suggests names, `scope.rename()` applies them).
- **Recursive splitting:** Clusters that exceed a size threshold are recursively sub-split. Target scale: 40k+ functions (Claude Code's unminified bundle).
- **Incremental delivery:** Layer 1 (library separation) ships first, then app code splitting.

## Key Concept: `exactHash`

The `exactHash` (from `src/analysis/structural-hash.ts`) is the foundation of cross-version stability. It's a 16-char hex SHA-256 of a **normalized** function AST where:

1. All identifier names → positional placeholders (`$0`, `$1`, ...) assigned in DFS order
2. String literals → `__STR_<length>__` (preserves length, not content)
3. Numeric literals → magnitude bucket (`Math.floor(Math.log10(|val| + 1))`)
4. All location data, comments, and `extra` fields stripped

**Why this is stable across versions:** Minifiers change variable names but preserve structure. Two minified versions of the same function produce identical `exactHash` values because identifiers are reduced to positional placeholders. Even across different minifiers (terser vs esbuild vs swc), structurally identical functions hash the same.

**What changes the hash:** Adding/removing a statement, changing control flow, adding a parameter. These are genuine code changes.

**Example:**
```javascript
// v1.0 (terser output)          // v1.1 (terser output, same function)
function a(b, c) {               function x(y, z) {
  if (b > 0) return c * 2;        if (y > 0) return z * 2;
  return c;                        return z;
}                                }
// Both produce exactHash: "7a3f2b1c4d5e6f8a" — identifiers are irrelevant
```

**Testing requirements:** We need extensive tests on hashing behavior to validate stability guarantees. See the Testing section for the test matrix.

## Architecture

### Pipeline Design

The project should support composable stages with well-defined interfaces:

```
Stage 1: Unpack    (webcrack)     → unpacked files + bundle metadata
Stage 2: Rename    (LLM + babel)  → humanified code + function graph + fingerprints
Stage 3: Split     (clustering)   → directory structure with imports/exports

# Each stage can run independently:
humanify unpack bundle.js -o unpacked/
humanify rename unpacked/ -o renamed/
humanify split renamed/ -o structured/

# Or combined:
humanify unminify bundle.js -o output/ --split
```

Each stage reads its predecessor's output + metadata sidecar. This enables:
- Iterating on splitting without re-running the expensive rename pass
- Swapping in different unpackers (webcrack alternatives)
- Running stages on different machines

### State Backend: Extending the Checkpoint DB

Per spec 16 (Resumable Processing), we already plan a SQLite checkpoint database at `<output-dir>/.humanify-checkpoint.db`. Rather than a separate JSON sidecar, we extend this database to store the function graph and split metadata. This gives us:

- Atomic writes (WAL mode) — no partial-write corruption
- Queryable structure (SQL for finding functions by hash, cluster membership, etc.)
- A single state backend for resumption, caching, and splitting
- Serializable: `sqlite3 .dump` produces readable SQL, or we can add a `humanify export-meta` command that emits JSON

**New tables for splitting:**

```sql
-- Function graph (populated during rename, consumed by split)
CREATE TABLE function_graph (
  fingerprint TEXT PRIMARY KEY,       -- exactHash (16-char hex)
  file_path TEXT NOT NULL,
  name TEXT,                          -- humanified name (post-rename)
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  arity INTEGER,
  complexity INTEGER,
  cfg_shape TEXT,
  external_calls TEXT,                -- JSON array of external call names
  FOREIGN KEY (file_path) REFERENCES file_checkpoint(file_path)
);

-- Call graph edges
CREATE TABLE function_edges (
  caller_fingerprint TEXT NOT NULL,
  callee_fingerprint TEXT NOT NULL,
  PRIMARY KEY (caller_fingerprint, callee_fingerprint),
  FOREIGN KEY (caller_fingerprint) REFERENCES function_graph(fingerprint),
  FOREIGN KEY (callee_fingerprint) REFERENCES function_graph(fingerprint)
);

-- Cluster assignments (populated during split)
CREATE TABLE cluster (
  cluster_id TEXT PRIMARY KEY,        -- hash of sorted member fingerprints
  parent_cluster_id TEXT,             -- for recursive sub-splitting
  name TEXT,                          -- LLM-suggested name
  path TEXT,                          -- output file/directory path
  FOREIGN KEY (parent_cluster_id) REFERENCES cluster(cluster_id)
);

CREATE TABLE cluster_member (
  fingerprint TEXT NOT NULL,
  cluster_id TEXT NOT NULL,
  is_root BOOLEAN NOT NULL DEFAULT FALSE,
  PRIMARY KEY (fingerprint, cluster_id),
  FOREIGN KEY (fingerprint) REFERENCES function_graph(fingerprint),
  FOREIGN KEY (cluster_id) REFERENCES cluster(cluster_id)
);

-- Cached LLM-suggested names (persists across runs for stability)
CREATE TABLE cluster_name_cache (
  cluster_id TEXT PRIMARY KEY,
  suggested_name TEXT NOT NULL,
  member_fingerprints TEXT NOT NULL,  -- JSON array, for fuzzy matching on changed clusters
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

## Layer 1: Library Separation (Milestone 1)

Move detected library code into `vendor/{libraryName}/` subdirectories.

### Input Signals

- `WebcrackFile.metadata.modulePath` containing `node_modules/` (webpack/browserify)
- `CommentRegion` data persisted in checkpoint DB (all bundlers)
- Library banner comments still present in deobfuscated output

### Algorithm

1. Read library detection data from checkpoint DB (or re-detect if running standalone on non-humanify output)
2. **Webpack/browserify:** Organize by recovered module path → `vendor/react/index.js`, `vendor/lodash/merge.js`, etc.
3. **Rollup/esbuild:** Extract comment-delimited library regions into `vendor/{libraryName}.js`. Library regions are byte-offset ranges in the file where a library banner was detected. Each region typically starts at a banner comment and ends at the next banner or EOF.
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

- **Library region boundaries vs function boundaries:** In Rollup output, library banner comments mark byte-offset regions, but function boundaries don't always align perfectly with region boundaries. We split at the function level — each function is classified as library or app based on which region its start position falls in. Functions are atomic units; they are never partially extracted.
- Multiple libraries interleaved (Rollup) → each gets its own file under vendor/
- No library detection data available → skip Layer 1, pass everything to Layer 2

## Layer 2: App Code Clustering (Milestone 2)

Split remaining app code into logical modules using fingerprint-stable dependency trees.

### Algorithm: Fingerprint-Stable Dependency Trees

**Step 1: Build or load function graph**

If running as part of the full pipeline, receive the already-built function graph from the rename step (same `FunctionNode[]` array). If running standalone, parse the deobfuscated code and call `buildFunctionGraph()`. Both paths produce the same data structure — the graph is serialized to/from the checkpoint DB so the split step doesn't need to know how it was built.

**Step 2: Identify root functions**

Root functions have `callers.size === 0` — nothing else in the file calls them. These are effectively the module's public API / exports.

**Step 3: Grow clusters by exclusive ownership**

```
for each root R:
  cluster[R] = { R }
  reachable[R] = BFS(R, following internalCallees edges)

for each non-root function F:
  roots_that_reach_F = { R : F in reachable[R] }
  if |roots_that_reach_F| == 1:
    assign F to that root's cluster
  else:
    assign F to shared cluster
```

**Step 4: Recursive sub-splitting**

If a cluster exceeds a size threshold, recursively sub-split it. The cluster becomes a directory, and its sub-clusters become files within it. The threshold should be informed by analyzing well-organized open source projects (see Testing section).

**Step 5: Cluster non-function declarations**

Top-level variables, classes, and constants are assigned to clusters using Babel scope analysis:
- Referenced by one cluster → belongs to that cluster
- Referenced by multiple clusters → shared module (name TBD — `_shared.js` is a placeholder, LLM can suggest a better name like `common.js` or `globals.js`)
- `const` declarations can be duplicated if small (< 100 chars) to reduce cross-file imports

**Step 6: Compute cluster identity**

```typescript
function clusterFingerprint(members: FunctionNode[]): string {
  const hashes = members.map(m => m.fingerprint.exactHash).sort();
  return sha256(hashes.join(':')).slice(0, 16);
}
```

This is deterministic and stable: same member functions → same cluster fingerprint.

### Cross-Version Stability: Worked Example

Consider a codebase with 6 functions across two versions:

```
v1.0 call graph:                    v1.1 call graph (added validateToken):

  createAuth ─→ hashPassword          createAuth ─→ hashPassword
       │                                    │
       └──→ generateToken                   ├──→ generateToken
                                            │
  formatDate (no callers)                   └──→ validateToken (NEW)

  parseConfig ─→ loadDefaults          formatDate (no callers)

                                        parseConfig ─→ loadDefaults
```

**v1.0 clusters:**
| Cluster | Root | Members | Cluster Fingerprint |
|---------|------|---------|-------------------|
| A | createAuth | {createAuth, hashPassword, generateToken} | hash(sort([exactHash_cA, exactHash_hP, exactHash_gT])) = `"c1a2b3..."` |
| B | formatDate | {formatDate} | hash([exactHash_fD]) = `"d4e5f6..."` |
| C | parseConfig | {parseConfig, loadDefaults} | hash(sort([exactHash_pC, exactHash_lD])) = `"a7b8c9..."` |

**v1.1 clusters:**
| Cluster | Root | Members | Cluster Fingerprint |
|---------|------|---------|-------------------|
| A' | createAuth | {createAuth, hashPassword, generateToken, **validateToken**} | `"e1f2a3..."` (changed — new member) |
| B' | formatDate | {formatDate} | `"d4e5f6..."` (identical to v1.0 B) |
| C' | parseConfig | {parseConfig, loadDefaults} | `"a7b8c9..."` (identical to v1.0 C) |

**Stability result:**
- Cluster B → B': Same fingerprint → same file, no diff
- Cluster C → C': Same fingerprint → same file, no diff
- Cluster A → A': Fingerprint changed, but **root identity** (createAuth's `exactHash`) is stable → same file name. Diff shows only the added `validateToken` function.

**Cross-version cluster matching:** When a cluster fingerprint changes (like A → A'), we match by member overlap. A and A' share 3/3 of A's original members (100% overlap) → same logical cluster → inherits A's cached name.

### Handling Circular Calls

In real JS codebases, circular dependencies between modules are common and handled via:
- **Barrel files / index.ts re-exports:** `auth/index.ts` re-exports from `auth/provider.ts` and `auth/token.ts`. The barrel calls nothing, but is called by external consumers.
- **Lazy resolution:** Module A imports module B and vice versa. Node.js resolves this via partial module objects at require-time.
- **Event systems:** Modules communicate via events rather than direct calls, breaking the call graph cycle.

**Our approach:** If two root functions call each other (circular), merge them into a single cluster. This is correct — they're tightly coupled and belong together. The merged cluster gets the fingerprint of all combined members.

```
if root A calls root B AND root B calls root A:
  merge cluster[A] and cluster[B]
  new root = the one with the lexicographically smaller exactHash (deterministic)
```

### Determinism Guarantees

- Sort by `exactHash` at every decision point
- Break ties with secondary sort on serialized `StructuralFeatures`
- Never depend on `Map`/`Set` iteration order for output-affecting decisions
- Use stable sort throughout (Node.js `Array.sort` is stable in V8)

## Layer 3: Import/Export Reconstruction

All code transforms in this layer are **mechanical Babel AST operations** — no LLM involvement. This is critical: just as renaming uses `scope.rename()` to apply LLM-suggested names, splitting uses Babel AST manipulation to move code between files.

### Mechanical Operations

The split is implemented as a sequence of atomic, deterministic operations:

```typescript
interface SplitOperation {
  type: 'extract-function' | 'extract-declaration' | 'add-export' | 'add-import' | 'remove-node';
  sourceFile: string;
  targetFile: string;
  nodeFingerprint?: string;     // for functions (identified by exactHash)
  nodeName?: string;            // for declarations (identified by binding name)
  exportName?: string;          // the name to export as
  importSource?: string;        // relative path for import statement
}
```

**Example operation sequence for splitting function `createAuth` into `auth.js`:**
```
1. extract-function  { source: "app.js", target: "auth.js", fingerprint: "c1a2b3..." }
2. extract-function  { source: "app.js", target: "auth.js", fingerprint: "d4e5f6..." }  // hashPassword
3. add-export        { target: "auth.js", exportName: "createAuth" }
4. add-import        { target: "app.js", importSource: "./auth.js", importName: "createAuth" }
5. remove-node       { source: "app.js", fingerprint: "c1a2b3..." }
6. remove-node       { source: "app.js", fingerprint: "d4e5f6..." }
```

### Babel Implementation

Each operation maps to a Babel AST transform:

- **extract-function:** `t.cloneNode(fnPath.node, true)` → insert into target AST
- **add-export:** Wrap existing declaration with `t.exportNamedDeclaration(declaration)`, or add `t.exportNamedDeclaration(null, [t.exportSpecifier(t.identifier(name), t.identifier(name))])`
- **add-import:** Insert `t.importDeclaration([t.importSpecifier(t.identifier(name), t.identifier(name))], t.stringLiteral(source))` at top of file
- **remove-node:** `fnPath.remove()` — Babel handles scope cleanup

All of these are standard Babel operations used across the JS ecosystem. They are deterministic and produce valid ASTs.

### Finding Cross-Cluster References

For identifying which functions/declarations are used across cluster boundaries, we use the existing `internalCallees`/`callers` edges from the function graph. For non-function declarations, Babel's `scope.getBinding()` provides reference tracking. No external graph library is needed — the function graph already has the edges, and Babel's scope system handles variable references.

For large graphs (40k+ functions), the cross-cluster reference check is O(E) where E = number of call graph edges. This is fast since we're just iterating edges and checking cluster membership.

### Shared Mutable State

Mutable (`let`/`var`) declarations referenced by multiple clusters → shared module:
```javascript
// common/state.js (name LLM-suggested)
export let currentUser = null;
```

**Simplification for v1:** Only split functions and `const` declarations. All mutable shared state stays in the shared module.

## Layer 4: LLM-Suggested Naming

### Algorithm

1. For each cluster, build a context summary:
   - Root function name(s) (already humanified from rename pass)
   - All exported function names
   - External API calls (`externalCallees`)
   - String literals from structural features
2. Send to LLM: *"Given these functions, suggest a file path like `src/auth/provider.js`"*
3. LLM responds with suggested path
4. Cache in `cluster_name_cache` table by cluster fingerprint

### Recursive Directory Naming

Since clusters are recursively sub-split, naming follows the hierarchy:

1. Top-level cluster → LLM suggests a **directory name** based on its functions (e.g., `auth/`)
2. Sub-clusters within it → LLM suggests **file names** (e.g., `auth/provider.js`, `auth/token.js`)
3. If a sub-cluster is itself split → it becomes a sub-directory, recurse

This produces natural hierarchies:
```
src/
  auth/
    provider.js         # createAuth, hashPassword
    token.js            # generateToken, validateToken
  config/
    parser.js           # parseConfig, loadDefaults
  utils/
    formatting.js       # formatDate
```

### Cross-Version Name Stability

Lookup strategy when processing a cluster:
1. **Exact match:** cluster fingerprint found in `cluster_name_cache` → use cached name
2. **Fuzzy match:** >70% member fingerprints overlap with a cached cluster → inherit cached name
3. **No match:** ask LLM for a new name, cache it

This ensures the LLM is only consulted for genuinely new clusters.

### Experimental Approach

The clustering + naming algorithm is fundamentally experimental. We should:
1. Build a test fixture set of well-organized open source projects
2. Minify them, run the full pipeline, compare output layout to original
3. Track metrics: cluster count, avg cluster size, naming accuracy, cross-version stability %
4. Iterate on the algorithm with data, not intuition

### Fallback Naming (no LLM available)

If running without LLM access (`humanify split --no-llm`):
- Use root function's humanified name: `createAuthProvider.js`
- If no clear root: truncated cluster fingerprint: `mod_a3f2b1c4.js`
- Directory names from common external calls: a cluster heavy on `fetch` → `api/`, heavy on DOM → `ui/`

## Observability

### Verbosity Levels

**`-v` (verbose):**
```
Split: 847 functions → 43 clusters (avg 19.7 functions/cluster)
  Roots identified: 38
  Shared functions: 52 → common/shared.js
  Library functions: 1,203 → vendor/
```

**`-vv` (debug):**
```
Cluster "auth" (root: createAuth [c1a2b3]):
  Members: createAuth, hashPassword, generateToken, validateToken
  Exports: createAuth (used by: cluster "app", cluster "api")
  Imports: parseConfig (from cluster "config")

Split operation: extract-function app.js → auth/provider.js [c1a2b3]
Split operation: add-export auth/provider.js createAuth
Split operation: add-import api/handler.js ./auth/provider.js createAuth
```

### Dry Run

`humanify split output/ --dry-run` produces a report without modifying any files:

```
Dry run — no files will be modified

Proposed structure:
  src/
    auth/
      provider.js       (4 functions, 127 lines)
      token.js          (3 functions, 89 lines)
    config/
      parser.js         (2 functions, 45 lines)
    api/
      handler.js        (8 functions, 234 lines)
    common/
      shared.js         (12 functions, 198 lines)
  vendor/
    react.js            (450 lines)
    lodash/             (6 files, 312 lines)

Operations: 43 extract-function, 38 add-export, 127 add-import

Cluster details written to: split-plan.json
```

The `split-plan.json` contains the full operation list so it can be inspected and even manually edited before applying.

## New Files

| File | Purpose |
|------|---------|
| `src/split/index.ts` | Main orchestrator, CLI integration |
| `src/split/types.ts` | Cluster, SplitPlan, SplitOperation types |
| `src/split/library-separator.ts` | Layer 1: extract library code to vendor/ |
| `src/split/cluster.ts` | Layer 2: fingerprint-stable dependency tree clustering |
| `src/split/import-export.ts` | Layer 3: Babel AST transforms for imports/exports |
| `src/split/naming.ts` | Layer 4: LLM-suggested file names with caching |
| `src/split/operations.ts` | Mechanical split operations (extract, move, add-import, etc.) |

## Modified Files

| File | Change |
|------|--------|
| `src/unminify.ts` | Persist function graph to checkpoint DB; optionally invoke split |
| `src/plugins/rename.ts` | Export function graph data for checkpoint DB |
| `src/index.ts` | Add `split` subcommand and `--split` flag |
| `src/analysis/types.ts` | Extend FunctionNode with optional cluster assignment |
| Checkpoint DB schema (spec 16) | Add `function_graph`, `function_edges`, `cluster`, `cluster_member`, `cluster_name_cache` tables |

## Existing Code to Reuse

| Module | Usage |
|--------|-------|
| `src/analysis/function-graph.ts` → `buildFunctionGraph()` | Call graph for clustering |
| `src/analysis/structural-hash.ts` → `computeExactHash()` | Cluster fingerprint computation |
| `src/analysis/function-fingerprint.ts` | Cross-version cluster matching |
| `src/library-detection/` (all layers) | Layer 1 library separation |
| `src/rename/processor.ts` | Pattern for LLM batching (reuse for naming) |
| Babel `scope.rename()`, `path.remove()`, AST builders | Layer 3 mechanical transforms |

## Testing

### exactHash Stability Test Matrix

Build a comprehensive test suite validating hash behavior:

| Test Case | Expected |
|-----------|----------|
| Same function, different variable names | Same hash |
| Same function, terser vs esbuild vs swc output | Same hash |
| Same function, different string literal content (same length) | Same hash |
| Same function, numeric literal 100 vs 999 (same magnitude) | Same hash |
| Function with added parameter | Different hash |
| Function with added if-branch | Different hash |
| Function with reordered statements | Different hash |
| Arrow function vs function expression (same body) | Different hash (different AST node type) |
| Nested function — inner function unchanged, outer changed | Inner hash unchanged |

### Clustering E2E Fixtures

Follow the existing pattern from `test/e2e/fixtures/` (mitt, nanoid, zustand). Create new fixtures specifically for split testing:

**Candidate projects (small, well-organized, public):**
1. **preact** — clear component/hook/dom module boundaries
2. **zustand** (already a fixture) — small state management lib with clear file structure
3. **date-fns** — many small, independent utility functions (good for testing many-cluster scenarios)
4. **express** middleware stack — tests deeply nested call chains

**Test process for each fixture:**
1. Clone the original repo, record the original file structure
2. Bundle with Rollup (scope-hoisted → single file)
3. Minify with terser
4. Run humanify rename
5. Run humanify split
6. Compare output directory structure against original
7. Repeat for v(N) and v(N+1), verify structural stability

### Cluster Size Thresholds

Analyze real projects to determine appropriate split thresholds:
- Sample 20+ well-organized open source projects
- Record: files per directory, functions per file, lines per file
- Derive p50/p90 thresholds for "when should we sub-split?"

### Dry-Run Validation

Every test case should first run in `--dry-run` mode and verify the plan matches expected output before applying.

## Open Questions

1. **Cluster size thresholds:** What's the right max-functions-per-file? Analyze popular open source projects to find the natural distribution of functions-per-file.

2. **Recursive sub-splitting algorithm:** When a cluster is too large, how do we sub-split? Options:
   - Re-run the same root-finding algorithm within the cluster
   - Split by scope nesting depth (top-level helpers vs deeply nested)
   - Split by sub-graph density (find natural sub-communities within the cluster)

3. **How to handle circular calls between roots:** Current proposal is merge. But if two 200-function clusters have a single bidirectional edge, merging produces a 400-function monster. Alternative: keep them separate, both export the cross-called functions, accept the circular import (Node.js handles it fine).

4. **Non-function top-level code:** What about top-level statements that aren't declarations? (e.g., `console.log("app started")`, `document.addEventListener(...)`). These likely belong in an entry-point file.

5. **Pipeline stage interfaces:** What's the formal contract between stages? Should each stage produce a well-defined manifest file, or is the checkpoint DB sufficient?

6. **Validation of split output:** Should we run `node --check` on each output file? Or go further and actually execute the split output to verify runtime equivalence with the original?
