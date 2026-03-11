# Structure Recovery

## Problem

The split pipeline clusters functions by call-graph reachability alone. This ignores critical structural information that bundlers preserve (or destroy in predictable ways):

1. **Mutable shared state** — Module-scoped `let`/`var` declarations that get reassigned end up in `shared.js`. Files that import them can't reassign them because ESM imports are immutable bindings. This is the `currentHook` bug: Preact's hook state variable must stay in the same file as the functions that mutate it.

2. **Intra-module statement ordering** — Scope-hoisting bundlers (Rollup, esbuild) concatenate modules in dependency order, preserving consecutive statement groups from original modules. The splitter ignores this contiguity signal.

3. **Bundler-specific boundary markers** — Webpack module wrappers, Rollup `$N` deconflict suffixes, IIFE/closure patterns all encode original module boundaries. These are lost when we treat the AST as a flat list of declarations.

4. **Scope patterns** — Top-level IIFEs, factory closures, and `__esModule` markers indicate module boundaries that the splitter doesn't recognize.

## Goals

1. Recover original module boundaries using bundler-specific and universal heuristics
2. Emit **constraints** that the clustering algorithm respects — not a separate splitting mechanism
3. Fix the ESM mutable binding bug (mutation affinity is the critical path)
4. Improve split quality by grouping related declarations that came from the same original module

## Non-Goals

- Perfect module boundary recovery (impossible without source maps)
- Replacing the clustering algorithm (constraints augment it, not replace it)
- Handling non-JavaScript bundler outputs (TypeScript, CSS-in-JS, etc.)
- Recovering original file names (that requires path information from unpacking)

## Architecture

### Constraint Model

All recovery heuristics produce **constraints** — declarative rules that the clustering algorithm must respect. Two types:

```typescript
interface ColocationConstraint {
  kind: "colocation";
  entries: string[];       // Ledger entry IDs that must be in the same output file
  reason: ColocationReason;
  source: ConstraintSource;
}

interface BoundaryConstraint {
  kind: "boundary";
  line: number;            // AST line number where a module boundary is detected
  strength: "hard" | "soft";
  source: ConstraintSource;
}

type ColocationReason =
  | "mutation-affinity"    // Variable + its mutators must co-locate (ESM correctness)
  | "webpack-module"       // Webpack wrapper = hard module boundary
  | "rollup-deconflict"   // Same $N suffix = same original module
  | "scope-closure"        // IIFE/closure = same module scope
  | "contiguity"           // Dense cross-references between consecutive statements
  | "deconflict-group";   // Grouped by deconflict suffix pattern

type ConstraintSource =
  | "mutation-analysis"
  | "bundler-heuristic"
  | "scope-analysis"
  | "contiguity-analysis";

type Constraint = ColocationConstraint | BoundaryConstraint;
```

### Integration via Union-Find

Before clustering, colocation constraints are processed into groups using a Union-Find (disjoint set) data structure:

1. Initialize each ledger entry as its own set
2. For each `ColocationConstraint`, union all entries in the constraint
3. The clustering algorithm operates on **groups** rather than individual entries
4. Any group member assigned to a cluster brings the entire group with it

This means the clustering algorithm doesn't need to understand constraints — it just sees larger atomic units.

### Priority Order for Conflicts

When constraints conflict (e.g., contiguity wants to separate entries that mutation affinity wants together):

1. **Mutation affinity** — Always wins. Correctness requirement: ESM imports are immutable.
2. **Hard boundary** — Definitive module boundary (webpack wrapper, IIFE).
3. **Deconflict group** — `$N` suffix pattern strongly indicates same original module.
4. **Scope closure** — Factory/closure patterns.
5. **Contiguity** — Statistical signal, may be overridden.
6. **Soft boundary** — Hints only, freely overridden.

Mutation affinity is non-negotiable because it prevents runtime errors. All other constraints affect output quality but not correctness.

### Pipeline Position

Structure recovery runs between AST parsing and clustering:

```
parse AST → recover structure (NEW) → cluster → assign ledger → emit
```

In `buildSplitPlan()`:

```typescript
const constraints = recoverStructure(ast, ledger, detection);
const groups = buildConstraintGroups(constraints, ledger);
const { clusters, shared, orphans } = clusterFunctions(allFunctions, {
  ...options,
  groups,
});
```

## Universal Heuristics

These heuristics apply regardless of bundler type.

### 1. Mutation Affinity (Critical Path)

This is the most important heuristic — it fixes the ESM mutable binding bug.

**Problem**: When `let counter = 0; function inc() { counter++; }` gets split so `counter` is in `shared.js` and `inc()` is in `file-1.js`, the import `import { counter } from './shared.js'` creates an immutable binding. `inc()` can't reassign `counter` because ESM `import` bindings are read-only.

**Algorithm**:

1. Find all top-level `let` and `var` declarations (these are potentially mutable)
2. For each declaration, walk all top-level functions looking for:
   - `AssignmentExpression` where the left-hand side references the variable
   - `UpdateExpression` (`++`, `--`) targeting the variable
3. Emit a `ColocationConstraint` grouping the variable declaration with all mutating functions
4. **Read-only references do NOT trigger co-location** — importing and reading an ESM binding is fine

```typescript
function analyzeMutationAffinity(
  ast: t.File,
  ledger: SplitLedger
): ColocationConstraint[] {
  const constraints: ColocationConstraint[] = [];

  for (const decl of findMutableTopLevelDeclarations(ast)) {
    const mutators = findMutatingFunctions(ast, decl.name);
    if (mutators.length > 0) {
      constraints.push({
        kind: "colocation",
        entries: [decl.ledgerId, ...mutators.map(m => m.ledgerId)],
        reason: "mutation-affinity",
        source: "mutation-analysis",
      });
    }
  }

  return constraints;
}
```

**`const` declarations**: `const` bindings cannot be reassigned, so they don't need mutation affinity. Property mutation on `const` objects (`const obj = {}; obj.x = 1`) is fine across ESM boundaries because the binding itself isn't reassigned.

### 2. Scope Pattern Analysis

Top-level structural patterns that indicate module boundaries:

**IIFEs**: A top-level IIFE (`(function() { ... })()` or `(() => { ... })()`) is a hard boundary. Everything inside the IIFE came from a single original module.

```typescript
// Hard boundary: entire IIFE is one module
(function() {
  var state = {};
  function init() { state.ready = true; }
  function getState() { return state; }
})();
```

**`__esModule` markers**: `Object.defineProperty(exports, '__esModule', { value: true })` indicates a CJS module boundary. The next occurrence marks the start of a new module.

**Factory closures**: A function that returns an object or another function, immediately invoked — the factory and its products should be co-located.

### 3. Contiguity Analysis (Experimental)

Consecutive statements with dense cross-references likely came from the same original module. Scope-hoisting preserves intra-module statement order.

**Algorithm**:

1. Sliding window of K statements (default K=5)
2. For each window position, compute a **density score**: ratio of cross-references within the window to total references from window statements
3. Detect "seams" where density drops sharply between adjacent windows
4. Emit colocation constraints for groups between seams

**Tunable parameters**:
- Window size K (experiment: sweep K=3,5,7,10)
- Density threshold for seam detection
- Minimum group size to emit a constraint

This heuristic is the most experimental and needs parameter tuning on real fixtures before being relied upon.

## Per-Bundler Heuristics

These require bundler detection (spec 24) and only run when the bundler type is known.

### 4. Webpack Module Recovery

Webpack bundles wrap each module in a function. After webcrack extraction, each module is already a separate file. This heuristic **validates** that extraction was correct:

- Each webpack module wrapper → hard boundary
- Module ID mappings → validate that cross-module references use `require()` or `import`
- Missing modules (tree-shaken) → log warning, don't create phantom boundaries

### 5. Rollup/esbuild Deconflict Analysis

Rollup and esbuild rename top-level declarations to avoid conflicts using `$N` suffixes:

```javascript
// Original module A: export function render() { ... }
// Original module B: export function render() { ... }
// After Rollup:
function render$1() { ... }  // from module A
function render$2() { ... }  // from module B
```

**Algorithm**:

1. Find all declarations with `$N` suffixes (regex: `/\$\d+$/`)
2. Group by suffix number: `{1: [render$1, state$1], 2: [render$2, state$2]}`
3. Each group → colocation constraint with reason `"deconflict-group"`

Declarations without `$N` suffixes either:
- Came from a module where no conflict existed (no grouping signal)
- Are the "winner" of a conflict (e.g., `render` without suffix = first module processed)

### 6. Browserify Module Recovery

Browserify bundles include a `require` map encoding module dependencies:

```javascript
{1: [function(require, module, exports) { ... }, {"./utils": 2}]}
```

The second element is the dependency map. This encodes module boundaries (each numeric key is a module) and inter-module relationships.

## Types

```typescript
// src/recovery/types.ts

interface ColocationConstraint {
  kind: "colocation";
  entries: string[];
  reason: ColocationReason;
  source: ConstraintSource;
}

interface BoundaryConstraint {
  kind: "boundary";
  line: number;
  strength: "hard" | "soft";
  source: ConstraintSource;
}

type ColocationReason =
  | "mutation-affinity"
  | "webpack-module"
  | "rollup-deconflict"
  | "scope-closure"
  | "contiguity"
  | "deconflict-group";

type ConstraintSource =
  | "mutation-analysis"
  | "bundler-heuristic"
  | "scope-analysis"
  | "contiguity-analysis";

type Constraint = ColocationConstraint | BoundaryConstraint;

interface RecoveryResult {
  constraints: Constraint[];
  /** Constraint groups after Union-Find processing */
  groups: Map<string, string[]>;  // group leader → member entry IDs
  /** Diagnostic info for logging */
  stats: {
    mutationConstraints: number;
    boundaryConstraints: number;
    scopeConstraints: number;
    contiguityConstraints: number;
    totalGroups: number;
  };
}
```

## File Structure

```
src/recovery/
  types.ts                     # Constraint types, RecoveryResult
  index.ts                     # Orchestrator: recoverStructure()
  union-find.ts                # Union-Find data structure for constraint groups
  mutation-affinity.ts         # Mutable variable + mutator co-location
  scope-analysis.ts            # IIFE, __esModule, factory patterns
  contiguity-analysis.ts       # Sliding window density analysis
  deconflict-analysis.ts       # Rollup/esbuild $N suffix grouping
  bundler/
    webpack.ts                 # Webpack module wrapper validation
    browserify.ts              # Browserify require map parsing
```

## Testing Approach

Implementation follows strict Red/Green TDD for each heuristic.

### Mutation Affinity Tests

1. **RED**: Test with `let counter = 0; function inc() { counter++; }` — assert colocation constraint emitted for `counter` + `inc`
2. **GREEN**: Implement `analyzeMutationAffinity()`
3. **RED**: Test with read-only reference `function get() { return counter; }` — assert NO constraint emitted for `get`
4. **GREEN**: Verify read-only exclusion logic
5. **RED**: Test with `const obj = {}; function set() { obj.x = 1; }` — assert NO constraint (const binding, property mutation is fine across ESM)
6. **GREEN**: Verify const exclusion
7. **RED**: Integration test — split code with mutable state, verify output bundles correctly with esbuild (round-trip test)
8. **GREEN**: Wire `analyzeMutationAffinity()` into `buildSplitPlan()`

### Scope Analysis Tests

1. **RED**: Test with top-level IIFE — assert hard boundary constraint
2. **GREEN**: Implement IIFE detection
3. **RED**: Test with `__esModule` marker — assert boundary constraint
4. **GREEN**: Implement marker detection

### Deconflict Analysis Tests

1. **RED**: Test with `render$1`, `state$1`, `render$2`, `state$2` — assert two colocation groups `{$1: [render$1, state$1], $2: [render$2, state$2]}`
2. **GREEN**: Implement deconflict grouping
3. **RED**: Test with declarations without `$N` suffix — assert no spurious grouping
4. **GREEN**: Verify exclusion

### Contiguity Analysis Tests

1. **RED**: Synthetic test with two known module blocks (dense internal refs, sparse cross-refs) — assert two colocation groups
2. **GREEN**: Implement sliding window analysis
3. **RED**: Test that unrelated adjacent statements with no cross-references don't get falsely grouped
4. **GREEN**: Verify density threshold

### Regression Tests

- **Preact fixture**: `currentHook` stays co-located with hook functions after mutation affinity
- **Constraint conflicts**: Deliberately conflicting inputs (mutation affinity vs contiguity), verify priority order holds

### Integration Tests

- **Union-Find**: Overlapping constraints merge correctly
- **Clustering**: Constrained groups are respected by clustering algorithm
- **Round-trip**: Split output bundles correctly with esbuild

## Implementation Phases

### Phase 1: Mutation Affinity (Critical Path)

- [ ] `ColocationConstraint` and `BoundaryConstraint` types
- [ ] Union-Find data structure
- [ ] `analyzeMutationAffinity()` — find mutable declarations and their mutators
- [ ] Wire into `buildSplitPlan()` via `recoverStructure()`
- [ ] Unit tests (mutable let, read-only const, property mutation on const)
- [ ] Integration test: Preact `currentHook` round-trip

### Phase 2: Scope Analysis

- [ ] IIFE detection → hard boundary constraints
- [ ] `__esModule` marker detection → boundary constraints
- [ ] Factory closure detection → colocation constraints
- [ ] Unit tests for each pattern

### Phase 3: Per-Bundler Heuristics

Requires spec 24 (bundler detection) to be implemented.

- [ ] Rollup/esbuild `$N` deconflict grouping
- [ ] Webpack module wrapper validation
- [ ] Browserify require map parsing
- [ ] Confusion matrix: each bundler's heuristic tested against other bundlers' output

### Phase 4: Contiguity Analysis (Experimental)

- [ ] Sliding window density computation
- [ ] Seam detection algorithm
- [ ] Parameter tuning on real fixtures (see Experiment 3)
- [ ] Integration with constraint system

## Experiments

### Experiment 1: `const` Mutation Across ESM Boundaries

- **Hypothesis**: `const obj = {}; obj.x = 1` works across ESM `import` boundaries because the binding itself isn't reassigned
- **Method**: Build a two-file fixture where file A exports `const obj = {}` and file B does `obj.x = 1`. Bundle with esbuild, verify it runs.
- **Expected outcome**: Works fine. Only `let`/`var` reassignment needs co-location.

### Experiment 2: Cycle Resolution vs Mutation Affinity

- **Hypothesis**: When `resolveImportCycles()` wants to move an entry that mutation affinity constrains, mutation affinity should win (correctness > cycle avoidance)
- **Method**: Build a fixture where a mutable variable participates in a cycle. Run splitting, verify mutation affinity is respected even if it means a cycle remains.
- **Decision**: If cycles with mutable state are common, consider alternative cycle-breaking strategies that respect constraints.

### Experiment 3: Contiguity Window Tuning

- **Hypothesis**: Window size K=5 with density threshold 0.6 provides good module boundary detection
- **Method**: Sweep K=3,5,7,10 and threshold=0.4,0.5,0.6,0.7 on the Preact fixture. Measure alignment with known module boundaries.
- **Expected outcome**: Optimal parameters depend on bundle size and density. Document the sweet spot.

### Experiment 4: Colocation vs `--min-cluster-size` Merging

- **Hypothesis**: Constrained groups should resist merging by `--min-cluster-size` (a mutation-affinity group of 2 entries shouldn't be merged into an unrelated cluster just because it's below the minimum)
- **Method**: Test with small constrained groups and aggressive `--min-cluster-size`. Verify groups stay intact.
- **Decision**: Mutation-affinity groups are always exempt from merging. Other constraint types may be merged.

### Experiment 5: Comment Regions as Boundary Constraints

- **Hypothesis**: Library detection `CommentRegion[]` (spec 05) can be piped into the constraint system as soft boundaries
- **Method**: Process a mixed bundle (React + app code) with comment regions, emit soft boundaries at region edges, verify structure recovery respects them
- **Decision**: If this improves split quality on mixed bundles, add it as a cross-spec integration point.

## Open Questions

1. **`const` mutation** → Experiment 1
2. **Cycle resolution conflict** → Experiment 2
3. **Contiguity window tuning** → Experiment 3
4. **Colocation + merging** → Experiment 4
5. **Comment regions as boundaries** → Experiment 5
6. **Constraint serialization**: Should constraints be persisted in the checkpoint DB (spec 16)? Useful for debugging but adds schema complexity.
7. **Visualization**: A constraint graph visualization would help debugging. Consider emitting a DOT file when `--diagnostics` is enabled.
