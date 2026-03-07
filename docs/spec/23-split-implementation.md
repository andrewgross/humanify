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
    ...
  _shared.js                 # functions/declarations used by multiple clusters
  _manifest.json             # metadata: cluster assignments, fingerprints, stats
```
>> i dont think we need a leading _ for these
>> also, _shared.js, that should just be another split off file, not sometthing special at the top level, it should be in src and is just another file. 


`_manifest.json` enables:
- Re-running with cached cluster names
- Diffing cluster assignments between versions
- Debugging clustering decisions

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
  "shared": {
    "outputPath": "_shared.js",
    "memberCount": 12
  },
  "stats": {
    "totalFunctions": 847,
    "totalClusters": 43,
    "avgClusterSize": 19.7,
    "sharedFunctions": 52
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
2. Identify roots: functions with callers.size === 0
3. For each root R:
     cluster[R] = {R}
     reachable[R] = BFS(R, following internalCallees)
4. For each non-root function F:
     owners = { R : F ∈ reachable[R] }
     if |owners| == 1 → assign F to that root's cluster
     if |owners| > 1  → assign F to _shared
     if |owners| == 0  → assign F to _orphans (shouldn't happen, but handle it)
5. Merge circular roots: if root A calls root B AND root B calls root A,
     merge their clusters (use lexicographically smaller exactHash as canonical root)
6. Compute cluster fingerprint: sha256(sorted member exactHashes).slice(0,16)
```

**Determinism rules:**
- All iteration over functions: sort by `exactHash`
- Tie-breaking: secondary sort on `sessionId` (positional, but deterministic for same input)
- Never depend on `Map`/`Set` iteration order for output-affecting decisions

**Validation approach:**
- Run on our existing e2e fixtures (mitt, nanoid, zustand) after unminifying them
- Manually inspect: do the clusters make sense?
- Compute Bunch's MQ metric as a quality diagnostic
- Count: how many functions end up in _shared? (too many = bad clustering)

>> we should identify some larger projects to use for these. I feel like we are going to allow something like 20 functions to exist in a single file, so these small e2e fixtures arent going to exercise the code much. Lets get a much larger repo to use.  Also, we should unminify it once, validate that we like the unminification, and then save that as a fixture for testing the clustering e2e.
>> actually, we will probably want to get two versions ( with code changes) for our new fixture(s)  and create output files for both, ideally re-using the function hashes so we keep the same names.
>> To start, we should view these as experiments to understand how to tune our clustering approach. We should be attempting to recording the input and outputs (perhaps in a git ignored experiments/ folder?) as well as our notes and reflections on the results so wec can refer to them later


### Phase 2: Mechanical Naming

Give clusters human-readable names without LLM involvement.

**Naming strategy (no LLM):**
1. If cluster has a single root function with a humanified name → use that name
   - `createAuth` root → `createAuth.js`
2. If cluster has multiple roots (merged circulars) → join names
   - `createAuth` + `validateAuth` → `auth.js` (common prefix extraction)
3. If no clear name → truncated cluster fingerprint
   - `mod_c1a2b3.js`

>> this is fine, though I expect that  we will end up having truncated fingerprints for the majority, we will see. That is fine, and we can add another step in the pipeline later for llm renaming of files/folders.

### Phase 3: Code Generation (import/export reconstruction)

Actually produce the output files with working import/export statements.

**Operations:**
1. For each cluster, create a new AST
2. Clone function nodes from source AST into target AST
3. For each cross-cluster reference:
   - Add `export` in the defining cluster's file
   - Add `import { name } from './other-cluster.js'` in the consuming file
   >> lets make sure these are very mechanical operations, what we dont want is to break the functionality of this code while refactoring like this
4. Handle non-function declarations:
   - `const`/`let`/`var` at module level → assign to cluster that uses them
   - Used by multiple clusters → put in `_shared.js`
   >> I dont love creating a _shared.js as a default, there are definitely cases where they would be something like constants or settings, but ideally these can fall out naturally from the code? I worry that everything will end up in _shared.js the way some of these minifiers work.
5. Write files using Babel's `generate()`

>>> explain more about how this works, dont we already have files at this point from steps 3 and 4?

**Babel operations needed:**
- `t.cloneNode(fnPath.node, true)` — deep clone function
- `t.exportNamedDeclaration(declaration)` — wrap in export
- `t.importDeclaration(specifiers, source)` — add import
- `path.remove()` — remove from source (only in the output copy)

### Phase 4: LLM-Suggested Naming

Replace mechanical names with semantically meaningful ones.

**Deferred — depends on:**
- Cluster quality being good enough that names are meaningful
- Understanding the right LLM prompt for file naming vs function naming

### Phase 5: Cross-Version Stability

Cache cluster assignments and names so v1.0 → v1.1 produces minimal diffs.

**Deferred — depends on:**
- Phase 1-3 working well on single versions
- Having two versions of a real codebase to test against

## What We Need From Upstream

### Currently working (can use as-is):
- `computeExactHash()` — 16-char structural hash, tested
- `buildFunctionGraph()` — call edges + callers, tested
- `FunctionNode.internalCallees` / `.callers` — the edges we need
- `FunctionNode.fingerprint.exactHash` — the stable identity

### Known limitations to be aware of:
- **exactHash collisions on small functions:** Very simple functions (getters, identity functions) may share the same hash. For clustering this is mostly fine — they'll get assigned by who calls them, not by their identity.
- **Call graph completeness:** Dynamic dispatch (`obj[method]()`) and higher-order functions (`arr.map(fn)`) may not be captured as edges. This means some relationships are invisible to clustering. Acceptable for v1; can improve later.
>>> as long as we dont lose these functions, it should be  fine for now, but we dont want to drop code.
- **Scope-hoisted code quirks:** After unminifying a Rollup bundle, all functions are at the top level with no module wrappers. `buildFunctionGraph()` should handle this fine since it works on call expressions, not module structure.

### Not needed for Phase 1:
- Checkpoint DB integration (Phase 5)
- Cross-version fingerprint matching (Phase 5)
>> lets keep this in mind as we structure our approach
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

>> make sure we are using Red Green TDD, write the test for a functionality first, run it and WATCH IT FAIL, then write the code that fixes the test. Repeat.

### Unit tests (Phase 1):
- `cluster.test.ts`:
  - Hand-crafted call graphs → verify cluster assignments
  - Circular dependencies → verify merging
  - All-shared graph (everything calls everything) → graceful degradation
  - Single function → single cluster
  - No functions → empty output
  - Determinism: run 100x, verify identical output

### Integration tests (Phase 1):
- Take existing e2e fixtures (mitt, nanoid, zustand)
- Unminify them (or use pre-unminified versions)
- Run clustering
>> these are going to be really small, so we want to find something bigger as well for testing

- Snapshot the cluster assignments (not the code, just "function X → cluster Y")

### Quality metrics to track:
- **Shared ratio:** `|_shared| / |total|` — lower is better (target: <15%)
- **MQ score:** Bunch's modularization quality — higher is better
- **Cluster count:** Should be reasonable for codebase size
- **Max cluster size:** No monster clusters
- **Single-function clusters:** Some are fine (utility functions), too many suggests over-splitting

## Open Questions for This Spec

1. **How do we handle nested functions?** A function defined inside another function is not a root even if nothing calls it (it's a closure). Should we skip nested functions and only cluster top-level ones? Or flatten the nesting?
>>> start with only top level ones for now and we can see how it goes when we are experimenting. Make sure that the output is something that you can review along side me (the metadata file should help with this.)


2. **Top-level non-function code:** `console.log("app started")`, IIFE wrappers, etc. These aren't functions with hashes. Probably goes in an `_entry.js` or `index.js`.
>>> probably, we should view how these get handled as part of our experiments

3. **What's a good max-cluster-size default?** Need data from real projects. Gut feeling: 20-30 functions per file is a reasonable ceiling.
>>> i think thats a good start, but we can analyze some alrger projects and see the distribution.

4. **Should _shared be one file or split by topic?** One big `_shared.js` is simple but could get large. Could sub-cluster the shared functions too.
>>> as i said before, i am worried this file will just end up with everything, ideally things fall out of the clustering naturally but we definitely dont want it getting to big.

5. **ESM vs CJS output?** The unminified input might use either. For now, match whatever the input uses. If input has no module syntax (scope-hoisted), default to ESM.
>>> sure, ESM is fine
