# Spec 23: File Splitting — Implementation Plan

Parent spec: [19-file-splitting.md](./19-file-splitting.md)
Research: [file-splitting-prior-art.md](../research/file-splitting-prior-art.md)

## Scope

This spec covers the **practical implementation** of file splitting as a standalone pipeline stage. Spec 19 covers the full vision; this spec is what we actually build first.

## Design Principles

1. **Pipeline stage, not monolith.** `humanify split` is a standalone command that reads already-unminified files and writes a new directory tree. It does not require the rename pass to have run in the same process.

2. **Input is sacred.** Never mutate input files. Output goes to a separate directory. This enables re-running with different parameters, A/B comparison, and debugging.

3. **Improvements upstream improve us.** The split step consumes `exactHash` and call graph edges as inputs. If those get more accurate, splitting automatically gets better. We don't bake in workarounds for their current limitations.

4. **Start simple, validate, iterate.** Ship the simplest thing that produces useful output. Measure quality. Then improve.

5. **No code left behind.** Every function and declaration in the input must appear in the output. Functions with incomplete call graph edges (dynamic dispatch, higher-order) still get assigned to clusters — they just might land in a suboptimal one.

6. **Experiment-driven tuning.** Clustering parameters and algorithm choices are validated through experiments on real codebases, with inputs, outputs, and observations recorded for reference.

## Pipeline Interface

```
Input:  one or more .js files (already unminified/renamed)
Output: a directory tree of .js files with import/export statements

humanify split <input-dir-or-file> -o <output-dir>

Options:
  --dry-run         Show proposed structure without writing files
  --max-cluster-size N   Max functions per file before sub-splitting (default: TBD)
  --no-llm          Use mechanical naming only (root function name or hash)
  --verbose / -v    Show clustering stats
  --debug / -vv     Show per-cluster details
```

### Input Contract

The split command accepts:
- A single `.js` file (scope-hoisted bundle output)
- A directory of `.js` files (webpack/browserify unpacked modules)

It parses the input fresh using Babel, calls `buildFunctionGraph()`, and works from there. No checkpoint DB required (though it can optionally read one for cached cluster names).

### Output Contract

```
<output-dir>/
  src/
    <cluster-name>.js        # or <cluster-name>/index.js for sub-split clusters
    shared.js                 # functions/declarations used by multiple clusters (just another file in src/)
    ...
  manifest.json              # metadata: cluster assignments, fingerprints, stats
```

The `shared.js` file is not special — it's just another file inside `src/` that happens to contain functions used by multiple clusters. Ideally, as clustering improves, fewer things end up here. If it grows too large, it gets sub-clustered like any other oversized cluster.

`manifest.json` enables:
- Re-running with cached cluster names
- Diffing cluster assignments between versions
- Debugging clustering decisions
- Human review of what the algorithm decided

```json
{
  "version": 1,
  "inputFiles": ["app.js"],
  "clusters": [
    {
      "id": "c1a2b3...",
      "name": "auth",
      "outputPath": "src/auth.js",
      "rootFunctions": ["createAuth"],
      "memberCount": 4,
      "memberHashes": ["abc123...", "def456...", "..."]
    }
  ],
  "stats": {
    "totalFunctions": 847,
    "totalClusters": 43,
    "avgClusterSize": 19.7,
    "sharedFunctions": 52,
    "sharedRatio": 0.061,
    "mqScore": 0.42
  }
}
```

## Implementation Phases

### Phase 1: Graph + Cluster (no code generation)

Build the clustering algorithm and validate it produces sensible groupings. Output is `--dry-run` only — a report of what clusters were found and what functions go where.

**What we build:**
- `src/split/cluster.ts` — core clustering algorithm
- `src/split/types.ts` — Cluster, SplitPlan types
- `src/split/index.ts` — CLI entry point (dry-run only)
- Tests against fixture files

**Algorithm (from spec 19, simplified):**

```
1. Parse input file(s), call buildFunctionGraph()
2. Identify roots: top-level functions with callers.size === 0
   (nested functions stay with their parent — only cluster top-level for now)
3. For each root R:
     cluster[R] = {R}
     reachable[R] = BFS(R, following internalCallees)
4. For each non-root function F:
     owners = { R : F ∈ reachable[R] }
     if |owners| == 1 → assign F to that root's cluster
     if |owners| > 1  → assign F to shared cluster
     if |owners| == 0  → assign F to orphan cluster (log a warning)
5. Merge circular roots: if root A calls root B AND root B calls root A,
     merge their clusters (use lexicographically smaller exactHash as canonical root)
6. Compute cluster fingerprint: sha256(sorted member exactHashes).slice(0,16)
```

**Determinism rules:**
- All iteration over functions: sort by `exactHash`
- Tie-breaking: secondary sort on `sessionId` (positional, but deterministic for same input)
- Never depend on `Map`/`Set` iteration order for output-affecting decisions

**Validation approach:**
- Use existing small e2e fixtures (mitt, nanoid, zustand) for basic sanity checks
- Find a larger real-world project (500+ functions) as the primary test fixture — unminify it once, validate the unminification, save the result as a fixture
- Get two versions of the larger fixture (with code changes between them) to test cross-version stability
- Record experiments in a gitignored `experiments/` directory with inputs, outputs, and notes on what we observed and what we'd change
- Manually inspect clusters: do the groupings make sense?
- Compute MQ metric as a quality diagnostic
- Track shared ratio: what % of functions end up in shared? (too many = bad clustering)

### Phase 2: Mechanical Naming

Give clusters human-readable names without LLM involvement. Expect most clusters to get fingerprint-based names at this stage — that's fine. LLM naming is a separate future pipeline step.

**Naming strategy (no LLM):**
1. If cluster has a single root function with a humanified name → use that name
   - `createAuth` root → `createAuth.js`
2. If cluster has multiple roots (merged circulars) → join names
   - `createAuth` + `validateAuth` → `auth.js` (common prefix extraction)
3. If no clear name → truncated cluster fingerprint
   - `mod_c1a2b3.js`

### Phase 3: Code Generation (import/export reconstruction)

Actually produce the output files with working import/export statements.

This is the riskiest phase — we're taking one file and splitting it into many while preserving runtime behavior. Every operation must be purely mechanical.

**How it works:**

After Phase 1+2, we have a plan: "function X goes to file Y." Phase 3 executes that plan:

1. Build a fresh AST for each output file (starts empty)
2. For each cluster, deep-clone its member functions from the source AST into the target file's AST
3. For each non-function top-level declaration (const/let/var):
   - Determine which cluster(s) reference it via Babel scope analysis
   - If referenced by one cluster → clone into that cluster's file
   - If referenced by multiple → clone into `shared.js` (or ideally, find a better home — see note below)
4. For each cross-cluster reference, add mechanical import/export:
   - `export` the declaration in the defining file
   - `import { name } from './defining-file.js'` in each consuming file
5. Generate output files from the ASTs using Babel's `generate()`

**On shared declarations:** The worry is that minified code often has many top-level declarations that end up referenced everywhere, making `shared.js` a dumping ground. To mitigate:
- Small `const` declarations (< 100 chars) can be duplicated into each consuming file instead of shared
- Declarations referenced by only 2 clusters could go into whichever cluster references them more
- We'll evaluate this during experiments and tune the heuristic

**Correctness guarantees:**
- Every function and declaration in the input appears exactly once in the output (or is explicitly duplicated with a comment noting why)
- No identifier references are broken — every cross-file reference has a matching import/export
- The combined output, if concatenated, would be equivalent to the input
- All operations use standard Babel AST transforms: `t.cloneNode()`, `t.exportNamedDeclaration()`, `t.importDeclaration()`, `path.remove()` — deterministic and well-understood

**Babel operations needed:**
- `t.cloneNode(fnPath.node, true)` — deep clone function
- `t.exportNamedDeclaration(declaration)` — wrap in export
- `t.importDeclaration(specifiers, source)` — add import
- `path.remove()` — remove from source (only in the output copy)

### Phase 4: LLM-Suggested Naming

Replace mechanical names with semantically meaningful ones. This is a separate pipeline step that can run after Phase 3.

**Deferred — depends on:**
- Cluster quality being good enough that names are meaningful
- Understanding the right LLM prompt for file naming vs function naming

### Phase 5: Cross-Version Stability

Cache cluster assignments and names so v1.0 → v1.1 produces minimal diffs.

**Deferred — depends on:**
- Phase 1-3 working well on single versions
- Having two versions of a real codebase to test against (planned as part of Phase 1 validation)

**Keep in mind during Phases 1-3:** Structure the code so that cluster identity (fingerprint of sorted member hashes) and manifest output are first-class concepts. This makes it straightforward to add caching and cross-version matching later without restructuring.

## What We Need From Upstream

### Currently working (can use as-is):
- `computeExactHash()` — 16-char structural hash, tested
- `buildFunctionGraph()` — call edges + callers, tested
- `FunctionNode.internalCallees` / `.callers` — the edges we need
- `FunctionNode.fingerprint.exactHash` — the stable identity

### Known limitations to be aware of:
- **exactHash collisions on small functions:** Very simple functions (getters, identity functions) may share the same hash. For clustering this is mostly fine — they'll get assigned by who calls them, not by their identity.
- **Call graph completeness:** Dynamic dispatch (`obj[method]()`) and higher-order functions (`arr.map(fn)`) may not be captured as edges. This means some relationships are invisible to clustering. Acceptable for v1; can improve later. Functions with missing edges still get assigned to a cluster (possibly as orphans or shared) — they are never dropped from the output.
- **Scope-hoisted code quirks:** After unminifying a Rollup bundle, all functions are at the top level with no module wrappers. `buildFunctionGraph()` should handle this fine since it works on call expressions, not module structure.

### Not needed for Phase 1 (but structure code to not preclude them):
- Checkpoint DB integration (Phase 5)
- Cross-version fingerprint matching (Phase 5)
- Library detection (can add as a pre-filter later)

## File Structure

```
src/split/
  index.ts          # CLI entry point, orchestrator
  types.ts          # Cluster, SplitPlan, SplitOperation
  cluster.ts        # Phase 1: root-finding + BFS clustering
  naming.ts         # Phase 2: mechanical naming, Phase 4: LLM naming
  codegen.ts        # Phase 3: import/export reconstruction
  quality.ts        # MQ metric + diagnostics
```

## Testing Strategy

**Red-Green TDD throughout:** Write the test first, run it, watch it fail, then write the code that makes it pass. Repeat.

### Unit tests (Phase 1):
- `cluster.test.ts`:
  - Hand-crafted call graphs → verify cluster assignments
  - Circular dependencies → verify merging
  - All-shared graph (everything calls everything) → graceful degradation
  - Single function → single cluster
  - No functions → empty output
  - Determinism: run 100x, verify identical output

### Integration tests (Phase 1):
- Small fixtures (mitt, nanoid, zustand) for basic sanity
- Larger fixture (500+ functions) as primary test target — need to identify and prepare this
- Snapshot the cluster assignments (not the code, just "function X → cluster Y")

### Experiments (ongoing):
- Gitignored `experiments/` directory
- Record: input file, clustering output (manifest.json), observations, parameter tweaks
- Compare two versions of same project to evaluate stability

### Quality metrics to track:
- **Shared ratio:** `|shared| / |total|` — lower is better (target: <15%)
- **MQ score:** Bunch's modularization quality — higher is better
- **Cluster count:** Should be reasonable for codebase size
- **Max cluster size:** No monster clusters
- **Single-function clusters:** Some are fine (utility functions), too many suggests over-splitting

## Open Questions

1. **Nested functions:** Start with top-level only. Nested functions stay with their parent. Revisit based on experiment results — the manifest output will make it easy to spot issues.

2. **Top-level non-function code:** `console.log("app started")`, IIFE wrappers, etc. Likely goes in an entry-point file. Evaluate during experiments.

3. **Max cluster size default:** Start with 20-30 functions per file. Analyze distribution in larger real-world projects to calibrate.

4. **Shared file growth:** Concerned this becomes a dumping ground. Mitigations: duplicate small consts, assign by majority reference. Evaluate during experiments — if >15% of functions land in shared, the clustering algorithm needs work.

5. **ESM vs CJS output:** Default to ESM.
