# Spec: Cross-Version Function Fingerprinting

## Problem Statement

When comparing two versions of a minified JavaScript program, we need to identify which functions in version B correspond to which functions in version A. This is challenging because:

1. **Variable names change** - minifiers rename all identifiers
2. **AST node IDs change** - Babel assigns different IDs based on parse order
3. **Position changes** - adding/removing code shifts line/column numbers
4. **Tree depth changes** - refactoring can move functions up/down in nesting

### Goal
Create a fingerprinting system that can identify the "same" function across different minification runs of evolving source code, enabling:
- Reuse of previously computed humanified names
- Tracking function evolution across versions
- Detecting when functions are genuinely new vs. modified vs. unchanged

### Non-Goals (for now)
- Fuzzy/approximate matching (Type-3/Type-4 clones)
- Cross-language matching
- Detecting copy-pasted code within same version

---

## Background Research

### Existing Approaches

| Approach | Cascade Behavior | Distinctiveness | Notes |
|----------|------------------|-----------------|-------|
| Merkle tree hash | Full cascade | High | One leaf change invalidates ancestors |
| CFG-only hash | No cascade | Medium | Doesn't capture call relationships |
| Feature vectors | No cascade | Medium-High | Bag of independent features |
| Topology-Aware Hashing | No cascade | High | N-gram graphical features |
| Weisfeiler-Lehman | Depth-limited | High | k-hop neighborhood hashing |

### Key Insight from Literature
[Topology-Aware Hashing](https://www.semanticscholar.org/paper/Topology-Aware-Hashing-for-Effective-Control-Flow-Li-Jang/1374a95c0568c8f2fd0391aea2aa76528b8508ed) extracts **n-gram features** from CFGs - local patterns that don't cascade when distant parts change.

---

## Proposed Solution: Puzzle-Piece Fingerprinting

### Core Metaphor

Each function is a **puzzle piece** with:
1. **Local Shape** - the internal structure (CFG, literals, arity)
2. **Connectors** - how it relates to neighbors (what it calls, what calls it)

Two functions match when their shapes match AND their connectors are compatible.

### Multi-Resolution Fingerprinting

```
Resolution 0: localHash only
Resolution 1: localHash + blurredCalleeShapes
Resolution 2: localHash + exactCalleeHashes + blurred2HopShapes
```

Higher resolutions are more distinctive but more sensitive to changes. Match at highest resolution possible, fall back to lower.

---

## Data Structures

### LocalFingerprint

The "shape" of a single function, independent of its callees.

```typescript
interface LocalFingerprint {
  // Primary hash (SHA-256 truncated) of normalized AST
  // Uses existing structural-hash.ts approach:
  // - Identifiers → positional placeholders ($0, $1)
  // - String literals → length markers (__STR_5__)
  // - Numbers → magnitude buckets
  hash: string;  // 16 hex chars

  // Decomposed features for fuzzy matching & disambiguation
  features: StructuralFeatures;
}

interface StructuralFeatures {
  // Signature
  arity: number;              // Parameter count
  hasRestParam: boolean;      // Uses ...rest

  // Complexity
  returnCount: number;        // Number of return statements
  complexity: number;         // Cyclomatic complexity estimate

  // Control flow shape
  cfgShape: string;           // e.g., "if-loop-if-return"
  loopCount: number;
  branchCount: number;
  tryCount: number;

  // Anchors (stable across minification)
  stringLiterals: string[];   // Sorted, deduplicated
  numericLiterals: number[];  // Sorted, deduplicated
  externalCalls: string[];    // ["fetch", "JSON.parse", "console.log"]
  propertyAccesses: string[]; // [".then", ".catch", ".length"]
}
```

### CalleeShape (Blurred Connector)

Describes a callee's structure without identifying it exactly.

```typescript
interface CalleeShape {
  arity: number;
  complexity: number;
  cfgType: 'linear' | 'branching' | 'looping' | 'complex';
  hasExternalCalls: boolean;
  // Intentionally excludes: callee's hash, callee's callees
}

// Serialized form for hashing: "(2,5,looping,true)"
```

### FunctionFingerprint (Complete)

```typescript
interface FunctionFingerprint {
  // === Resolution 0: Local only ===
  localHash: string;
  features: StructuralFeatures;

  // === Resolution 1: 1-hop blurred ===
  // Callee shapes (blurred - no cascade if callee internals change)
  calleeShapes: CalleeShape[];      // Sorted for determinism
  callerShapes: CalleeShape[];      // Optional: who calls me
  externalCallees: string[];        // ["fetch", "setTimeout"]

  // === Resolution 2: 1-hop exact + 2-hop blurred ===
  calleeHashes: string[];           // Exact localHashes of callees
  twoHopShapes: string[];           // Shapes of callees' callees
}
```

### FingerprintIndex

For efficient lookup across a codebase.

```typescript
interface FingerprintIndex {
  // Primary index: localHash → sessionIds
  byLocalHash: Map<string, string[]>;

  // Secondary: (localHash, calleeShapesHash) → sessionIds
  byResolution1: Map<string, string[]>;

  // Full fingerprints for detailed comparison
  fingerprints: Map<string, FunctionFingerprint>;  // sessionId → fingerprint
}
```

---

## Algorithms

### 1. Computing Local Fingerprint

Extends existing `structural-hash.ts`:

```typescript
function computeLocalFingerprint(node: t.Function): LocalFingerprint {
  // Existing: normalize and hash
  const hash = computeStructuralHash(node);

  // New: extract structural features
  const features = extractStructuralFeatures(node);

  return { hash, features };
}

function extractStructuralFeatures(node: t.Function): StructuralFeatures {
  const features: StructuralFeatures = {
    arity: node.params.length,
    hasRestParam: node.params.some(p => t.isRestElement(p)),
    returnCount: 0,
    complexity: 1,  // Base complexity
    cfgShape: '',
    loopCount: 0,
    branchCount: 0,
    tryCount: 0,
    stringLiterals: [],
    numericLiterals: [],
    externalCalls: [],
    propertyAccesses: [],
  };

  // Single traversal to collect all features
  traverse(node, {
    ReturnStatement() { features.returnCount++; },
    IfStatement() { features.branchCount++; features.complexity++; },
    ConditionalExpression() { features.branchCount++; features.complexity++; },
    SwitchStatement() { features.branchCount++; features.complexity++; },
    ForStatement() { features.loopCount++; features.complexity++; },
    WhileStatement() { features.loopCount++; features.complexity++; },
    DoWhileStatement() { features.loopCount++; features.complexity++; },
    ForInStatement() { features.loopCount++; features.complexity++; },
    ForOfStatement() { features.loopCount++; features.complexity++; },
    TryStatement() { features.tryCount++; },
    StringLiteral(path) { features.stringLiterals.push(path.node.value); },
    NumericLiteral(path) { features.numericLiterals.push(path.node.value); },
    MemberExpression(path) {
      if (t.isIdentifier(path.node.property)) {
        features.propertyAccesses.push('.' + path.node.property.name);
      }
    },
    CallExpression(path) {
      // Collect external/native calls
      const callee = path.node.callee;
      if (t.isIdentifier(callee) && isLikelyExternal(callee.name)) {
        features.externalCalls.push(callee.name);
      }
      if (t.isMemberExpression(callee) && t.isIdentifier(callee.property)) {
        features.externalCalls.push(callee.property.name);
      }
    },
  });

  // Build CFG shape string
  features.cfgShape = buildCfgShapeString(node);

  // Sort and deduplicate arrays
  features.stringLiterals = [...new Set(features.stringLiterals)].sort();
  features.numericLiterals = [...new Set(features.numericLiterals)].sort((a,b) => a-b);
  features.externalCalls = [...new Set(features.externalCalls)].sort();
  features.propertyAccesses = [...new Set(features.propertyAccesses)].sort();

  return features;
}
```

### 2. Building CFG Shape String

A compact representation of control flow structure.

```typescript
function buildCfgShapeString(node: t.Function): string {
  const shapes: string[] = [];

  // Walk body statements in order, record control flow
  function walkStatements(statements: t.Statement[]) {
    for (const stmt of statements) {
      if (t.isIfStatement(stmt)) {
        shapes.push('if');
        if (stmt.consequent) walkBlock(stmt.consequent);
        if (stmt.alternate) {
          shapes.push('else');
          walkBlock(stmt.alternate);
        }
      } else if (t.isForStatement(stmt) || t.isWhileStatement(stmt) ||
                 t.isForOfStatement(stmt) || t.isForInStatement(stmt)) {
        shapes.push('loop');
        walkBlock(stmt.body);
      } else if (t.isTryStatement(stmt)) {
        shapes.push('try');
        if (stmt.handler) shapes.push('catch');
        if (stmt.finalizer) shapes.push('finally');
      } else if (t.isReturnStatement(stmt)) {
        shapes.push('ret');
      } else if (t.isSwitchStatement(stmt)) {
        shapes.push('switch');
      } else if (t.isThrowStatement(stmt)) {
        shapes.push('throw');
      }
    }
  }

  function walkBlock(node: t.Statement | t.BlockStatement) {
    if (t.isBlockStatement(node)) {
      walkStatements(node.body);
    } else {
      walkStatements([node]);
    }
  }

  if (t.isBlockStatement(node.body)) {
    walkStatements(node.body.body);
  } else {
    // Arrow function with expression body
    shapes.push('expr');
  }

  return shapes.join('-') || 'empty';
}
```

### 3. Computing Callee Shapes (Blurred Connectors)

```typescript
function computeCalleeShape(callee: FunctionNode): CalleeShape {
  const features = extractStructuralFeatures(callee.path.node);

  return {
    arity: features.arity,
    complexity: features.complexity,
    cfgType: classifyCfgType(features),
    hasExternalCalls: features.externalCalls.length > 0,
  };
}

function classifyCfgType(features: StructuralFeatures): string {
  if (features.loopCount > 0 && features.branchCount > 0) return 'complex';
  if (features.loopCount > 0) return 'looping';
  if (features.branchCount > 0) return 'branching';
  return 'linear';
}

function serializeCalleeShape(shape: CalleeShape): string {
  return `(${shape.arity},${shape.complexity},${shape.cfgType},${shape.hasExternalCalls})`;
}
```

### 4. Building Full Fingerprint

```typescript
function buildFunctionFingerprint(
  fn: FunctionNode,
  graph: Map<string, FunctionNode>
): FunctionFingerprint {
  const local = computeLocalFingerprint(fn.path.node);

  // Resolution 1: Blurred callee shapes
  const calleeShapes = [...fn.internalCallees]
    .map(computeCalleeShape)
    .sort((a, b) => serializeCalleeShape(a).localeCompare(serializeCalleeShape(b)));

  const callerShapes = [...fn.callers]
    .map(computeCalleeShape)
    .sort((a, b) => serializeCalleeShape(a).localeCompare(serializeCalleeShape(b)));

  // Resolution 2: Exact callee hashes
  const calleeHashes = [...fn.internalCallees]
    .map(c => computeLocalFingerprint(c.path.node).hash)
    .sort();

  // Resolution 2: Two-hop shapes (callees' callees)
  const twoHopShapes: string[] = [];
  for (const callee of fn.internalCallees) {
    for (const calleeOfCallee of callee.internalCallees) {
      twoHopShapes.push(serializeCalleeShape(computeCalleeShape(calleeOfCallee)));
    }
  }
  twoHopShapes.sort();

  return {
    localHash: local.hash,
    features: local.features,
    calleeShapes,
    callerShapes,
    externalCallees: [...fn.externalCallees].sort(),
    calleeHashes,
    twoHopShapes: [...new Set(twoHopShapes)],
  };
}
```

### 5. Matching Functions Across Versions

```typescript
function matchFunctions(
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): MatchResult {
  const matches = new Map<string, string>();      // oldId → newId
  const ambiguous = new Map<string, string[]>();  // oldId → candidate newIds
  const unmatched: string[] = [];                 // oldIds with no match

  for (const [oldId, oldFp] of oldIndex.fingerprints) {
    // Try Resolution 0: exact localHash match
    const candidates = newIndex.byLocalHash.get(oldFp.localHash) || [];

    if (candidates.length === 0) {
      unmatched.push(oldId);
      continue;
    }

    if (candidates.length === 1) {
      matches.set(oldId, candidates[0]);
      continue;
    }

    // Multiple candidates - try Resolution 1: blurred callee shapes
    const r1Candidates = candidates.filter(newId => {
      const newFp = newIndex.fingerprints.get(newId)!;
      return calleeShapesMatch(oldFp.calleeShapes, newFp.calleeShapes);
    });

    if (r1Candidates.length === 1) {
      matches.set(oldId, r1Candidates[0]);
      continue;
    }

    if (r1Candidates.length > 1) {
      // Try Resolution 2: exact callee hashes
      const r2Candidates = r1Candidates.filter(newId => {
        const newFp = newIndex.fingerprints.get(newId)!;
        return arraysEqual(oldFp.calleeHashes, newFp.calleeHashes);
      });

      if (r2Candidates.length === 1) {
        matches.set(oldId, r2Candidates[0]);
        continue;
      }
    }

    // Still ambiguous
    ambiguous.set(oldId, r1Candidates.length > 0 ? r1Candidates : candidates);
  }

  return { matches, ambiguous, unmatched };
}

function calleeShapesMatch(a: CalleeShape[], b: CalleeShape[]): boolean {
  if (a.length !== b.length) return false;
  const aStrs = a.map(serializeCalleeShape);
  const bStrs = b.map(serializeCalleeShape);
  return arraysEqual(aStrs, bStrs);
}
```

---

## N-gram Call Graph Extension

### Concept

Traditional n-grams work on sequences. For call graphs, we use **edge-centric n-grams**:

- **Unigram**: Single node's localHash
- **Bigram**: Edge `(caller.localHash → callee.localHash)`
- **Trigram**: Path `(A.hash → B.hash → C.hash)`

### Edge N-gram Structure

```typescript
interface EdgeNgram {
  // Bigram: direct call relationship
  caller: string;  // localHash
  callee: string;  // localHash OR serialized CalleeShape for blurred version
}

interface PathNgram {
  // Trigram+: call chain
  path: string[];  // [A.hash, B.hash, C.hash, ...]
}
```

### Computing Edge N-grams

```typescript
function computeEdgeNgrams(
  fn: FunctionNode,
  mode: 'exact' | 'blurred'
): string[] {
  const myHash = computeLocalFingerprint(fn.path.node).hash;

  return [...fn.internalCallees].map(callee => {
    const calleeId = mode === 'exact'
      ? computeLocalFingerprint(callee.path.node).hash
      : serializeCalleeShape(computeCalleeShape(callee));

    return `${myHash}→${calleeId}`;
  });
}

function computePathNgrams(
  fn: FunctionNode,
  depth: number  // 2 = trigrams, 3 = 4-grams, etc.
): string[] {
  const paths: string[] = [];
  const myHash = computeLocalFingerprint(fn.path.node).hash;

  function walk(current: FunctionNode, path: string[], remaining: number) {
    if (remaining === 0) {
      paths.push(path.join('→'));
      return;
    }

    for (const callee of current.internalCallees) {
      const calleeHash = computeLocalFingerprint(callee.path.node).hash;
      walk(callee, [...path, calleeHash], remaining - 1);
    }
  }

  walk(fn, [myHash], depth);
  return paths;
}
```

### Using N-grams for Matching

N-grams provide **additional discriminating power** without full cascade:

```typescript
interface EnhancedFingerprint extends FunctionFingerprint {
  // Edge n-grams (bigrams)
  exactEdges: string[];    // ["abc→def", "abc→ghi"]
  blurredEdges: string[];  // ["abc→(2,3,loop,true)"]

  // Path n-grams (trigrams) - optional, for disambiguation
  pathTrigrams: string[];  // ["abc→def→xyz"]
}
```

**Cascade Behavior:**

| N-gram Type | If leaf changes... |
|-------------|-------------------|
| Unigram (localHash) | Only leaf changes |
| Bigram (exact) | Leaf + direct callers change |
| Bigram (blurred) | Only leaf changes (if shape stable) |
| Trigram | Leaf + 1-hop + 2-hop callers change |

**Recommendation**: Use blurred bigrams as default, exact bigrams for disambiguation.

---

## Integration with Existing Codebase

### Files to Modify/Create

1. **`src/analysis/structural-hash.ts`** - Extend with feature extraction
2. **`src/analysis/function-fingerprint.ts`** (NEW) - Full fingerprinting logic
3. **`src/analysis/fingerprint-index.ts`** (NEW) - Index building and matching
4. **`src/analysis/function-graph.ts`** - Add fingerprint computation during graph build

### Integration Points

```typescript
// In function-graph.ts buildFunctionGraph()
export function buildFunctionGraph(
  ast: t.File,
  filePath: string,
  options?: { computeFingerprints?: boolean }
): FunctionGraph {
  // ... existing graph building ...

  if (options?.computeFingerprints) {
    for (const [sessionId, fn] of functions) {
      fn.fingerprint = buildFunctionFingerprint(fn, functions);
    }
  }

  return { functions, roots, hasLocalFunctions };
}
```

### Caching Strategy

```typescript
// Cache structure for cross-version matching
interface FingerprintCache {
  version: string;  // Cache format version
  sourceHash: string;  // Hash of input file(s)

  // Function fingerprints by sessionId
  fingerprints: Record<string, SerializedFingerprint>;

  // Humanified names by fingerprint localHash
  names: Record<string, HumanifiedNames>;
}
```

---

## Cascade Analysis Summary

When a **leaf function** changes:

| Component | Resolution 0 | Resolution 1 (blurred) | Resolution 1 (exact) | Resolution 2 |
|-----------|--------------|------------------------|----------------------|--------------|
| Leaf | ❌ Changed | ❌ Changed | ❌ Changed | ❌ Changed |
| Direct caller | ✅ Stable | ✅ Stable* | ❌ Changed | ❌ Changed |
| 2-hop caller | ✅ Stable | ✅ Stable | ✅ Stable | ❌ Changed |
| 3-hop+ caller | ✅ Stable | ✅ Stable | ✅ Stable | ✅ Stable |

*Stable if leaf's calleeShape (arity, complexity, cfgType) is unchanged.

**Recommended Default**: Resolution 1 with blurred callee shapes - balances distinctiveness with stability.

---

## Testing Strategy

### Unit Tests

1. **Feature extraction** - Verify correct counts for arity, complexity, loops, etc.
2. **CFG shape strings** - Test various control flow patterns
3. **Callee shape computation** - Verify blurring works correctly
4. **Fingerprint equality** - Same function with different names → same fingerprint

### Integration Tests

1. **Same code, different minifiers** - Should match
2. **Same code, different formatting** - Should match
3. **Modified function** - Should not match (different localHash)
4. **Added function in middle** - Other functions should still match
5. **Renamed callee** - Caller should still match (blurred shapes stable)

### Test Cases

```typescript
// Test: Renamed identifiers produce same fingerprint
const codeA = `function foo(a, b) { return a + b; }`;
const codeB = `function x(y, z) { return y + z; }`;
// fingerprint(codeA) === fingerprint(codeB)

// Test: Different structure produces different fingerprint
const codeC = `function foo(a, b) { return a * b; }`;
// fingerprint(codeA) !== fingerprint(codeC)

// Test: Callee change doesn't affect caller (blurred)
const v1 = `
  function leaf() { return 1; }
  function caller() { return leaf() + 1; }
`;
const v2 = `
  function leaf() { return 2; }  // Changed!
  function caller() { return leaf() + 1; }
`;
// fingerprint(v1.caller).localHash === fingerprint(v2.caller).localHash
// fingerprint(v1.caller).calleeShapes === fingerprint(v2.caller).calleeShapes
```

---

## Open Questions

1. **How to handle closures?** - Should captured variables affect the fingerprint?
2. **How to handle method calls?** - `obj.method()` vs `method()` treatment
3. **Should we fingerprint class methods separately?** - Or treat class as unit
4. **Performance** - Is computing all features too slow for large codebases?
5. **Storage** - How much space do fingerprints take? Need compression?

---

## Implementation Phases

### Phase 1: Enhanced Local Fingerprinting
- Extract structural features alongside existing hash
- Build CFG shape strings
- Unit tests for feature extraction

### Phase 2: Callee Shape Computation
- Implement blurred connector computation
- Integrate with function graph building
- Test cascade behavior

### Phase 3: Multi-Resolution Matching
- Build fingerprint index
- Implement matching algorithm
- Integration tests with real minified code

### Phase 4: N-gram Extensions (Optional)
- Add edge/path n-grams
- Evaluate if they improve matching accuracy
- Performance testing

---

## References

- [Topology-Aware Hashing for CFG Similarity](https://www.semanticscholar.org/paper/Topology-Aware-Hashing-for-Effective-Control-Flow-Li-Jang/1374a95c0568c8f2fd0391aea2aa76528b8508ed)
- [QBinDiff - Graph Alignment for Binary Diffing](https://www.deepbits.com/blog/DeepDiff)
- [SIGMADIFF - Deep Graph Matching (NDSS 2024)](https://www.ndss-symposium.org/wp-content/uploads/2024-208-paper.pdf)
- [0xdevalias - JS Library Fingerprinting Notes](https://gist.github.com/0xdevalias/31c6574891db3e36f15069b859065267)
- [JSidentify-V2 - Obfuscation-Resistant Fingerprinting](https://arxiv.org/html/2508.01655)
