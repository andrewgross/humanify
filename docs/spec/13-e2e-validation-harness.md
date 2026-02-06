# Spec: End-to-End Fingerprint Validation Harness

## Problem Statement

We have built a cross-version fingerprinting system (spec 12) and a structural hashing system (spec 03) that are designed to identify when a function is "the same" across different minified versions of a codebase. We need a way to validate that these systems actually work correctly against real-world code.

Specifically, we need to answer:
1. When a function is **unchanged** between v1 and v2, does our fingerprinting correctly identify it as reusable (cache hit)?
2. When a function is **modified**, does our fingerprinting correctly identify it as needing fresh processing (cache miss)?
3. When a function is **new** in v2, does our system correctly treat it as unmatched?
4. When a function is **removed** in v1, does it avoid false matches?

### Why This Matters

If fingerprinting is too loose, we reuse stale cached names for changed functions (incorrect output). If too strict, we waste LLM calls re-processing unchanged functions (wasted cost/time). We need empirical evidence of where we land.

## Approach

We control the entire pipeline to create a closed-loop test:

1. Obtain **source code** for two versions of a real open-source package
2. **Minify the source ourselves** with configurable minifier tools/settings
3. Derive **ground truth** by structurally diffing the original source ASTs (since we have full source, we know exactly what changed)
4. Run humanify's fingerprinting on the minified outputs
5. Compare fingerprint matching results against ground truth
6. Report metrics and generate debug artifacts for mismatches

```
Source v1 ──┬──► Minify ──► v1.min.js ──┐
            │                           │
            ├──► AST Diff ──► Ground    ├──► Humanify ──► Compare ──► Report
            │               Truth       │    Fingerprint
Source v2 ──┴──► Minify ──► v2.min.js ──┘
```

## Starter Package: Mitt

We need a small, well-structured package to iterate quickly. **mitt** (https://github.com/developit/mitt) is a good first candidate:

- ~100 lines of TypeScript source
- Pure library, no dependencies
- Clear function boundaries (mitt, on, off, emit, all.clear)
- Multiple tagged releases with incremental changes
- Single-file output after build

Other candidates to add later: nanoid (~200 LOC), zustand (~500 LOC), preact (~3k LOC).

The harness itself is package-agnostic — mitt is just the first fixture.

## Directory Structure

```
test/e2e/
├── harness/
│   ├── index.ts                 # CLI entry point
│   ├── setup.ts                 # Download/prepare source fixtures
│   ├── minify.ts                # Minification with configurable tools
│   ├── ground-truth.ts          # AST diff to determine what changed
│   ├── validate.ts              # Run fingerprinting + compare to truth
│   ├── reporter.ts              # Output results (CI + human-readable)
│   └── debug.ts                 # Investigate specific mismatches
├── fixtures/
│   └── mitt/
│       ├── fixture.config.json  # Package metadata + version pairs
│       ├── source/
│       │   ├── v3.0.0/          # Original source (from git tag)
│       │   └── v3.0.1/
│       └── minified/
│           ├── v3.0.0/
│           │   └── terser-default.js
│           └── v3.0.1/
│               └── terser-default.js
├── snapshots/
│   └── mitt/
│       └── v3.0.0-v3.0.1-terser-default.snapshot.json
└── output/                      # Generated during test runs (gitignored)
    └── mitt/
        └── v3.0.0-v3.0.1-terser-default/
            ├── results.json
            ├── mismatches.json
            └── debug/
```

## Data Structures

### Fixture Configuration

Each test fixture describes a package and which version pairs to test.

```typescript
interface FixtureConfig {
  // Package identity
  package: string;                // "mitt"
  repo: string;                   // "https://github.com/developit/mitt"

  // How to obtain source for a version
  sourceStrategy:
    | { type: 'git-tag'; tagPattern: string }   // e.g., "v{version}" or "{version}"
    | { type: 'git-commit'; commits: Record<string, string> };  // version → commit SHA

  // Which file(s) to minify
  entryPoints: string[];          // ["src/index.ts"]

  // Build step if needed (e.g., compile TS before minifying)
  buildCommand?: string;          // "npx tsc src/index.ts --outDir build"

  // Version pairs to test
  versionPairs: Array<{
    v1: string;
    v2: string;
    description?: string;         // Human-readable note about what changed

    // Functions where source-level modification produces identical minified output.
    // These are explicitly documented syntactic-only changes where fingerprint
    // matching is the correct/expected behavior (not a failure).
    expectMatchDespiteModification?: Array<{
      function: string;           // Function name
      reason: string;             // Why the match is expected
    }>;
  }>;
}
```

Example for mitt:

```json
{
  "package": "mitt",
  "repo": "https://github.com/developit/mitt",
  "sourceStrategy": {
    "type": "git-tag",
    "tagPattern": "{version}"
  },
  "entryPoints": ["src/index.ts"],
  "versionPairs": [
    {
      "v1": "3.0.0",
      "v2": "3.0.1",
      "description": "Patch release"
    }
  ]
}
```

### Minifier Configuration

Minification is configurable so we can test whether our fingerprinting is robust across different minifier behaviors.

```typescript
interface MinifierConfig {
  id: string;                     // "terser-default"
  tool: 'terser' | 'esbuild' | 'uglify' | 'swc';
  options: Record<string, unknown>;
}
```

We start with a single config and expand later:

```typescript
const DEFAULT_CONFIGS: MinifierConfig[] = [
  {
    id: 'terser-default',
    tool: 'terser',
    options: {
      compress: true,
      mangle: true,
    }
  },
  // Phase 3: add esbuild, swc, etc.
];
```

### Ground Truth

Ground truth is derived by comparing the **source** ASTs of v1 and v2. Since we have the full original source, we can determine exactly which functions changed.

```typescript
interface GroundTruth {
  v1Functions: SourceFunction[];
  v2Functions: SourceFunction[];
  correspondence: FunctionCorrespondence[];
}

interface SourceFunction {
  // Identity in source
  id: string;                     // Stable ID: "file.ts::functionName"
  name: string;                   // Original function name from source
  file: string;                   // Relative file path
  location: { startLine: number; endLine: number };

  // Structural info (for matching across versions)
  structuralHash: string;         // Hash of normalized AST (same algo humanify uses)
  bodyHash: string;               // Hash of just the function body
  arity: number;                  // Parameter count
}

interface FunctionCorrespondence {
  // The source function identity
  sourceName: string;
  sourceFile: string;

  // Presence in each version
  inV1: boolean;
  inV2: boolean;

  // Classification
  changeType:
    | 'unchanged'                 // Identical source in both versions
    | 'modified'                  // Same function, different implementation
    | 'added'                     // Only in v2
    | 'removed';                  // Only in v1

  // If modified, what changed
  changeDetails?: {
    signatureChanged: boolean;
    bodyChanged: boolean;
    diff?: string;                // Unified diff snippet for debugging
  };
}
```

### Validation Results

```typescript
interface ValidationResult {
  // Test identity
  fixture: string;                // "mitt"
  v1: string;                     // "3.0.0"
  v2: string;                     // "3.0.1"
  minifierConfig: string;         // "terser-default"
  timestamp: string;

  // Counts
  v1FunctionCount: number;
  v2FunctionCount: number;
  groundTruthCorrespondences: number;

  // Core metrics, broken down by change type
  metrics: {
    // Unchanged functions: fingerprints SHOULD match
    unchangedFunctions: {
      total: number;
      fingerprintsMatched: number;      // Correct — cache would be reused
      fingerprintsMismatched: number;   // Bug — unnecessary cache miss
    };

    // Modified functions: fingerprints SHOULD differ
    modifiedFunctions: {
      total: number;
      fingerprintsDiffered: number;     // Correct — would trigger fresh LLM call
      fingerprintsMatched: number;      // Risky — stale cache reuse
    };

    // Added functions: should have no match in v1
    addedFunctions: {
      total: number;
      noMatchFound: number;             // Correct
      falseMatchFound: number;          // Bug — matched wrong v1 function
    };

    // Removed functions: validated by absence in v2 matching
    removedFunctions: {
      total: number;
    };
  };

  // Summary scores
  cacheReuseAccuracy: number;       // % of unchanged correctly matched
  changeDetectionAccuracy: number;  // % of modified correctly identified as different
  overallAccuracy: number;          // Combined score

  // Individual failures for debugging
  failures: ValidationFailure[];
}
```

### Validation Failures (Debug Detail)

```typescript
interface ValidationFailure {
  type:
    | 'unchanged-but-fingerprint-mismatch'   // Should have matched, didn't
    | 'modified-but-fingerprint-match'        // Changed but fingerprint same
    | 'added-but-false-match'                 // New function matched an old one
    | 'unexpected-match';                     // Matched the wrong function

  // What function
  sourceName: string;
  sourceFile: string;

  // What we expected vs got
  expected: string;
  actual: string;

  // Fingerprint details
  v1Fingerprint?: FunctionFingerprint;
  v2Fingerprint?: FunctionFingerprint;
  fingerprintDiff?: FingerprintDiff;

  // Code for human inspection
  v1SourceCode?: string;
  v2SourceCode?: string;
  v1MinifiedCode?: string;
  v2MinifiedCode?: string;
}

interface FingerprintDiff {
  localHashMatch: boolean;
  featuresMatch: boolean;
  calleeShapesMatch: boolean;

  // Specific field differences
  differences: string[];          // ["loopCount: 1 vs 2", "arity: 2 vs 3"]
}
```

## Algorithms

### Ground Truth Extraction

We determine function correspondence by comparing the original source ASTs.

```typescript
function buildGroundTruth(v1Source: ParsedFile[], v2Source: ParsedFile[]): GroundTruth {
  const v1Functions = v1Source.flatMap(f => extractFunctions(f));
  const v2Functions = v2Source.flatMap(f => extractFunctions(f));

  const correspondence: FunctionCorrespondence[] = [];
  const matchedV2Ids = new Set<string>();

  for (const v1Fn of v1Functions) {
    // Step 1: Try exact match — same name, same file
    let v2Match = v2Functions.find(f =>
      f.name === v1Fn.name && f.file === v1Fn.file && !matchedV2Ids.has(f.id)
    );

    // Step 2: If no name match, try structural match (function was renamed in source)
    if (!v2Match) {
      v2Match = v2Functions.find(f =>
        f.bodyHash === v1Fn.bodyHash && !matchedV2Ids.has(f.id)
      );
    }

    if (v2Match) {
      matchedV2Ids.add(v2Match.id);

      // Determine if function body changed
      const bodyChanged = v1Fn.bodyHash !== v2Match.bodyHash;
      const signatureChanged = v1Fn.arity !== v2Match.arity;

      correspondence.push({
        sourceName: v1Fn.name,
        sourceFile: v1Fn.file,
        inV1: true,
        inV2: true,
        changeType: (bodyChanged || signatureChanged) ? 'modified' : 'unchanged',
        changeDetails: (bodyChanged || signatureChanged) ? {
          signatureChanged,
          bodyChanged,
        } : undefined,
      });
    } else {
      correspondence.push({
        sourceName: v1Fn.name,
        sourceFile: v1Fn.file,
        inV1: true,
        inV2: false,
        changeType: 'removed',
      });
    }
  }

  // Find added functions (in v2 but unmatched)
  for (const v2Fn of v2Functions) {
    if (!matchedV2Ids.has(v2Fn.id)) {
      correspondence.push({
        sourceName: v2Fn.name,
        sourceFile: v2Fn.file,
        inV1: false,
        inV2: true,
        changeType: 'added',
      });
    }
  }

  return { v1Functions, v2Functions, correspondence };
}
```

### Validation Logic

After computing fingerprints for both minified versions, we compare against ground truth.

```typescript
function validate(
  groundTruth: GroundTruth,
  v1Fingerprints: Map<string, FunctionFingerprint>,
  v2Fingerprints: Map<string, FunctionFingerprint>,
  matchResult: MatchResult    // Output of our cross-version matching algorithm
): ValidationResult {
  const failures: ValidationFailure[] = [];

  for (const corr of groundTruth.correspondence) {
    switch (corr.changeType) {
      case 'unchanged': {
        // Our fingerprints SHOULD have matched these functions
        const matched = findMatchForFunction(corr, matchResult);
        if (!matched) {
          failures.push({
            type: 'unchanged-but-fingerprint-mismatch',
            sourceName: corr.sourceName,
            sourceFile: corr.sourceFile,
            expected: 'Fingerprints should match (function unchanged)',
            actual: 'No fingerprint match found',
            // ... attach fingerprints and code
          });
        }
        break;
      }

      case 'modified': {
        // Our fingerprints SHOULD differ (depending on what changed)
        const matched = findMatchForFunction(corr, matchResult);
        if (matched && corr.changeDetails?.bodyChanged) {
          // If the body changed significantly, matching might be wrong
          failures.push({
            type: 'modified-but-fingerprint-match',
            sourceName: corr.sourceName,
            sourceFile: corr.sourceFile,
            expected: 'Fingerprints should differ (function modified)',
            actual: 'Fingerprints matched despite modification',
          });
        }
        break;
      }

      case 'added': {
        // Should NOT match any v1 function
        const falseMatch = findFalseMatchForAdded(corr, matchResult);
        if (falseMatch) {
          failures.push({
            type: 'added-but-false-match',
            sourceName: corr.sourceName,
            sourceFile: corr.sourceFile,
            expected: 'No match (function is new in v2)',
            actual: `Falsely matched to v1 function: ${falseMatch}`,
          });
        }
        break;
      }

      case 'removed':
        // No action needed — validated by v2 side
        break;
    }
  }

  return buildResult(groundTruth, failures);
}
```

### Linking Minified Functions to Source Functions

A key challenge: after minification, we need to know which minified function corresponds to which source function. Since we minify the source ourselves, we can do this by:

1. **Position mapping**: Minifiers generally preserve function order. We can parse both source and minified ASTs, extract functions in order, and correlate by index.
2. **Source map from minifier**: When we run the minifier, we request a source map. The source map tells us exactly which minified position maps to which source position.
3. **Structural matching**: As a fallback, match by structural hash between source (pre-minification) normalized AST and minified normalized AST.

**Recommended approach**: Option 2 (source map from minifier). When we minify, we always generate a source map. This gives us a direct mapping from each minified function back to its source location, which we can use to link minified functions to our ground truth.

```typescript
interface MinificationResult {
  code: string;                   // Minified code
  sourceMap: RawSourceMap;        // Maps minified → source positions
}

function linkMinifiedToSource(
  minifiedFunctions: MinifiedFunction[],
  sourceFunctions: SourceFunction[],
  sourceMap: SourceMapConsumer
): Map<string, string> {                // minifiedId → sourceId
  const links = new Map();

  for (const mFn of minifiedFunctions) {
    // Use source map to find original position
    const originalPos = sourceMap.originalPositionFor({
      line: mFn.location.startLine,
      column: mFn.location.startColumn,
    });

    if (originalPos.source && originalPos.line) {
      // Find source function at that position
      const sourceFn = sourceFunctions.find(sFn =>
        sFn.file === originalPos.source &&
        sFn.location.startLine <= originalPos.line &&
        sFn.location.endLine >= originalPos.line
      );

      if (sourceFn) {
        links.set(mFn.id, sourceFn.id);
      }
    }
  }

  return links;
}
```

## Debug Output

When validation fails, we generate detailed artifacts to help diagnose the issue.

### Debug Directory Structure

```
output/mitt/v3.0.0-v3.0.1-terser-default/debug/
├── ground-truth.json             # Full ground truth data
├── v1-fingerprints.json          # All v1 fingerprints
├── v2-fingerprints.json          # All v2 fingerprints
├── matching-log.json             # Step-by-step matching decisions
├── failures/
│   └── emit-unchanged-mismatch/
│       ├── summary.txt           # Human-readable explanation
│       ├── v1-source.js          # Original source
│       ├── v2-source.js
│       ├── v1-minified.js        # Minified version
│       ├── v2-minified.js
│       ├── v1-fingerprint.json   # Detailed fingerprint
│       ├── v2-fingerprint.json
│       └── fingerprint-diff.txt  # Field-by-field comparison
```

### Failure Summary Format

```
===============================================
FAILURE: unchanged-but-fingerprint-mismatch
Function: emit (src/index.ts)
===============================================

EXPECTED: Fingerprints should match (function unchanged in source)
ACTUAL:   Fingerprints differ

SOURCE DIFF:
  (no changes — function is identical in v1 and v2 source)

FINGERPRINT COMPARISON:
  localHash:       abc123...  vs  def456...  ✗ MISMATCH
  arity:           2          vs  2          ✓
  complexity:      3          vs  3          ✓
  loopCount:       1          vs  1          ✓
  branchCount:     1          vs  1          ✓
  cfgShape:        if-loop-ret  vs  if-loop-ret  ✓
  stringLiterals:  ["*"]      vs  ["*"]      ✓
  calleeShapes:    [...]      vs  [...]      ✓

ROOT CAUSE ANALYSIS:
  localHash differs but all extracted features match.
  The structural hash may be sensitive to AST details that
  changed due to minifier behavior, not source changes.

MINIFIED CODE:
  v1: function n(e,t){var r=o[e];r&&r.slice().map(function(n){n(t)...
  v2: function n(e,t){var r=o[e];r&&r.slice().forEach(function(n){n(t)...

RECOMMENDATION:
  Investigate whether .map() vs .forEach() is a minifier optimization
  or an actual source change not captured by ground truth.
```

## CLI Commands

```bash
# Set up fixtures: clone repo, checkout tags, extract source
npm run e2e -- setup mitt

# Run validation for a specific version pair + minifier config
npm run e2e -- validate mitt 3.0.0 3.0.1
npm run e2e -- validate mitt 3.0.0 3.0.1 --minifier terser-default

# Run all configured version pairs for a fixture
npm run e2e -- validate mitt --all

# Update CI snapshots with current results
npm run e2e -- validate mitt 3.0.0 3.0.1 --update-snapshot

# Compare against existing snapshot (CI mode — fail on mismatch)
npm run e2e -- validate mitt --all --ci

# Debug a specific function mismatch
npm run e2e -- debug mitt 3.0.0 3.0.1 --function emit

# List available fixtures and their status
npm run e2e -- list
```

## CI Integration

```yaml
# .github/workflows/e2e.yml
e2e-validation:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-node@v4
    - run: npm ci
    - run: npm run e2e -- setup mitt
    - run: npm run e2e -- validate mitt --all --ci
    # --ci flag: compare against committed snapshots, fail on drift
```

## Implementation Phases

### Phase 1: Minimal Viable Harness
- Setup script to clone mitt repo and checkout source for two tagged versions
- Single minifier config (terser, default settings, with source map output)
- Ground truth extraction: parse source ASTs, diff to classify functions
- Link minified functions to source via minifier-generated source map
- Run humanify fingerprinting on both minified files
- Basic pass/fail output with counts

### Phase 2: Reporting & Debug Tooling
- Detailed metrics output (ValidationResult)
- Per-failure debug artifact generation
- Human-readable summary format
- Snapshot comparison for CI

### Phase 3: Expand Coverage
- Multiple minifier configs (esbuild, swc, uglify)
- Additional fixtures (nanoid, zustand)
- More version pairs per fixture

### Phase 4: Advanced Tooling
- Interactive debug mode for investigating mismatches
- HTML report generation
- Fingerprint diff visualization
- Aggregate metrics across all fixtures/configs

## Syntactic-Only Modifications

Some source-level changes don't affect the minified structural output. For example:

- **Arrow → function declaration**: `let foo = () => { ... }` → `function foo() { ... }` minifies to the same structure
- **Inline → explicit variable**: `(all.get(type) || []).slice().map(...)` → `let h = all.get(type); if (h) { h.slice().map(...) }` can collapse back to the same minified form under aggressive compression

These are classified as "modified" by ground truth (the source body hash differs), but fingerprint matching is **correct** to treat them as the same function. This is desirable for cache reuse — the function's semantics haven't changed, so reusing the cached humanified name is the right behavior.

To codify this, version pairs in `fixture.config.json` support `expectMatchDespiteModification`: an array of `{ function, reason }` entries that document which functions are expected to match despite source-level modification, and why. These are tracked separately in the `syntacticOnly` metric and counted as correct rather than as failures.

This ensures that:
1. The decision is explicitly documented per-function with a reason
2. If fingerprinting behavior changes (e.g. the function stops matching), the test will detect the regression
3. New unexpected matches still surface as failures until explicitly reviewed

## Open Questions

1. **TypeScript source**: Mitt is TypeScript. We need to compile to JS before minifying (or use esbuild/swc which handle TS directly). The build step should be part of the fixture config.

2. **Scope of "function"**: Do we count arrow functions, class methods, and IIFEs? For ground truth, we should match whatever humanify considers a "function" during fingerprinting.

3. **Export wrappers**: Bundlers/minifiers may wrap code in IIFEs or module patterns. These wrapper functions aren't "real" application functions. We may need to exclude them from validation.

4. **Determinism**: We should verify that running the same minifier on the same input always produces the same output, otherwise our tests will be flaky.
