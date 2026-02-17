# Fingerprint Benchmarking

## Overview

This spec compares humanify's multi-resolution fingerprinting to two academic approaches — DEBUN's Property-Order Graphs (ASE 2025) and PTdetector's property trees (ASE 2023) — and defines a benchmarking methodology for evaluating library detection.

## Side-by-Side Comparison

### Humanify: Normalized AST Hash + Call Graph Shapes

**How it works**: Each function is fingerprinted by normalizing its AST (replacing identifiers with positional placeholders, stripping comments and `extra` metadata), hashing the result, and recording the shapes/hashes of its internal callees.

**Multi-resolution matching**:
- Resolution 0: `exactHash` only (normalized AST structure)
- Resolution 1: `exactHash` + blurred `calleeShapes` (arity, complexity, cfg type)
- Resolution 2: `exactHash` + exact `calleeHashes` + `twoHopShapes`

**What survives minification**: Identifiers are replaced during normalization anyway, so minified names don't matter. Control flow structure, literal values, property accesses, and call graph topology are preserved.

**Strengths**:
- Multi-resolution allows trading precision for recall
- Call graph topology provides strong disambiguation
- Already integrated with humanify's rename pipeline (cache reuse across versions)

**Weaknesses**:
- Requires functions to have internal callees for Resolution 1+ matching
- Structural hash is sensitive to AST-level transforms (e.g., arrow → function, CJS → ESM)
- No automated feature selection — feature dimensions are manually defined

### DEBUN: Property-Order Graphs (POG)

**How it works**: For each function, DEBUN records the sequence of property operations (reads, writes, calls on object properties) in execution order. This ordered sequence forms a Property-Order Graph. Functions are fingerprinted by comparing their POGs.

**Key insight**: Bundlers (webpack, rollup, etc.) preserve property names and operation ordering even after transpilation. `obj.foo()` stays as a property access on `foo` regardless of what `obj` is renamed to.

**What survives minification**: Property names, string literals, operation ordering. Does NOT survive: identifier names (expected), control flow restructuring (rare in bundlers).

**Evaluation**: 68 high-traffic websites, 78 libraries. 91.76% F1-score for library detection, 79.81% for version detection.

**Strengths**:
- Very high bundler resilience — POGs are invariant to webpack/rollup transforms
- Works on any function with property operations (broader than call-graph approaches)
- Static analysis — no runtime needed

**Weaknesses**:
- Functions without property operations are invisible
- Requires building a reference database of POGs for known library versions
- Version detection is harder than library detection (79.81% vs 91.76%)

### PTdetector: Property Trees (pTree)

**How it works**: At runtime, PTdetector captures the property relationship tree for each loaded library — the tree of `object.property` relationships as they exist in memory. Features are automatically extracted from these trees using random vertex removal, root-pruning, and credit assignment.

**Key insight**: Runtime property trees are a natural, stable fingerprint. Libraries create characteristic property structures (`React.Component`, `React.createElement`, `React.useState`) that persist regardless of how the code is bundled.

**What survives minification**: Everything that exists at runtime — property names, prototype chains, module exports. The runtime state is the ground truth.

**Evaluation**: 200 top-traffic websites, 4000+ libraries from CDNJS. 100% precision, 98.1% recall (at threshold 68).

**Strengths**:
- Fully automated feature generation (no manual curation)
- Highest reported accuracy (100% precision)
- Can detect webpack-bundled libraries that other Chrome extensions miss
- Runtime analysis bypasses all static obfuscation

**Weaknesses**:
- Requires runtime execution (browser instrumentation)
- Cannot work on static code analysis alone
- Runtime overhead for feature collection
- Libraries must be loaded to be detected (tree-shaken code is invisible)

## What Each Approach Exploits

| Property of minified code | Humanify | DEBUN | PTdetector |
|---------------------------|----------|-------|------------|
| AST structure (control flow, nesting) | Primary | - | - |
| Property access names | Via `StructuralFeatures.propertyAccesses` | Primary | Primary |
| Property operation ordering | - | Primary | - |
| Call graph topology | Primary (calleeShapes/Hashes) | - | - |
| String/numeric literals | Via `StructuralFeatures` | Partial | - |
| Runtime object structure | - | - | Primary |
| Function arity/complexity | Via `StructuralFeatures` + `CalleeShape` | - | - |
| External call names | Via `StructuralFeatures.externalCalls` | Partial | - |

## Where We're Stronger vs. Weaker

### Humanify is stronger for:
- **Cross-version cache reuse**: Our multi-resolution matching is designed for finding "same function, different version" to reuse LLM rename results. DEBUN/PTdetector identify libraries but don't map individual functions across versions.
- **Function-level granularity**: We fingerprint every function individually. DEBUN operates at function-level too, but PTdetector is library-level.
- **Offline/static analysis**: No runtime needed, unlike PTdetector.

### Academic approaches are stronger for:
- **Library identification at scale**: Both DEBUN and PTdetector are evaluated on 68-200 real websites with dozens of libraries. We have 3 fixtures with 21 snapshots.
- **Bundler resilience**: DEBUN's POGs are specifically designed to survive bundler transforms. Our structural hashes can break on CJS→ESM or arrow→function conversions.
- **Automated feature selection**: PTdetector's feature generation is fully automated. We manually define what goes into `StructuralFeatures`.
- **Property-based detection**: Both DEBUN and PTdetector exploit property names as a primary signal. We use property accesses only as one sub-feature in `StructuralFeatures`.

## Proposed Benchmark Methodology

### Dataset Construction

**Option A: CDNJS Reference Builds**
1. Select 20-30 popular libraries from CDNJS (React, Vue, Angular, lodash, moment, axios, redux, etc.)
2. For each library, take 2-3 versions (e.g., React 17, 18, 19)
3. Create minimal apps importing each library
4. Bundle with our 3 minifier configs (terser, esbuild, swc)
5. Ground truth: we know exactly which library is in each bundle

**Option B: HTTP Archive Bundles**
1. Download production bundles from HTTP Archive top sites (publicly available via BigQuery)
2. Cross-reference with Wappalyzer detection data (also in HTTP Archive)
3. Manual verification for ground truth on a subset
4. More realistic but harder to control

**Recommended**: Start with Option A for controlled evaluation, then validate on a small set from Option B.

### Ground Truth Construction

For Option A bundles:
- Source maps + `package.json` provide exact library→function mapping
- Build unbundled reference fingerprints from library source
- Map bundled functions back to reference via source maps

For Option B bundles:
- Use Wappalyzer + Library Detector Chrome as initial signals
- Cross-reference with `package.json` when available in source maps
- Manual verification on disagreements

### Test Matrix

```
Libraries: [react, react-dom, vue, angular, lodash, moment, axios, redux, zustand, ...]
Versions:  [latest, latest-1, latest-2] per library
Bundlers:  [webpack, rollup, esbuild]
Minifiers: [terser, esbuild, swc]
Configs:   [default, aggressive, with-source-maps]
```

This gives us a matrix of `~30 libs × 3 versions × 3 bundlers × 3 minifiers = ~810` test cases.

### Metrics

**Library-level detection**:
- **Precision**: Of files classified as library X, how many actually contain library X?
- **Recall**: Of files that contain library X, how many were correctly detected?
- **F1-score**: Harmonic mean of precision and recall

**Function-level detection** (humanify-specific):
- **Skip accuracy**: Of functions skipped as "library", what % were actually library code?
- **Skip coverage**: Of all library functions, what % were correctly skipped?
- **False skip rate**: Application functions incorrectly skipped (must be near 0)

**Performance**:
- Detection time per file
- Memory usage for fingerprint database

### Comparison Protocol

To compare against DEBUN/PTdetector fairly:

1. **Reproduce their evaluation**: Use similar dataset sizes and library counts
2. **Normalize metrics**: All use precision/recall/F1 at library level
3. **Control for bundler**: Test same bundles across all approaches
4. **Report per-category**: Results for "easy" libraries (jQuery, React with banners) vs "hard" libraries (utility libs, polyfills)

## Extending the E2E Harness

The existing `test/e2e/` harness uses `.fptest.ts` files with snapshot baselines. To support library detection benchmarks:

### New Test Type: `.libtest.ts`

```typescript
// test/e2e/library-detection/react-webpack.libtest.ts
import { defineLibraryTest } from "../harness";

export default defineLibraryTest({
  name: "react-webpack-terser",
  bundle: "fixtures/react-app-webpack-terser.min.js",
  expectedLibraries: {
    "react": { files: ["react.js"], confidence: "high" },
    "react-dom": { files: ["react-dom.js"], confidence: "high" },
  },
  expectedNovel: ["app.js", "components/*.js"],
});
```

### Fixture Generation Script

```bash
# Generate test bundles from template apps
npm run generate-fixtures -- --libraries react,lodash --bundlers webpack,rollup --minifiers terser,esbuild,swc
```

This would:
1. Create minimal apps importing each library
2. Bundle with each bundler/minifier combination
3. Save bundles + source maps as test fixtures
4. Generate ground truth from source maps

### Integration with Existing Tests

- Library detection tests run as part of `npm run test:fingerprint`
- New snapshot format: `test/e2e/snapshots/library-detection/*.snap`
- Same `--update-snapshot` flag to update baselines

## Concrete Next Steps

1. **Build reference fingerprint database**: Fingerprint the top 10 libraries (unminified source) and store as test fixtures
2. **Create controlled test bundles**: 10 libraries × 3 minifiers = 30 bundles with known ground truth
3. **Implement basic detection benchmark**: Measure precision/recall of our Layer 1 (path matching) and Layer 2 (comment/banner) detection
4. **Add property-access features**: Enhance `StructuralFeatures` with ordered property access sequences (moving toward DEBUN's approach)
5. **Evaluate on HTTP Archive**: Download 50 production bundles and test detection against Wappalyzer ground truth
6. **Publish results**: Document precision/recall/F1 in a benchmark results file, updated as detection improves

## References

- DEBUN: "Detecting Bundled JavaScript Libraries on Web using Property-Order Graphs" (ASE 2025). Seojin Kim, Sungmin Park, Jihyeok Park.
- PTdetector: "An Automated JavaScript Front-end Library Detector" (ASE 2023). Aaron Liu et al. [GitHub](https://github.com/aaronxyliu/PTdetector)
- "Thou Shalt Not Depend on Me: Analysing the Use of Outdated JavaScript Libraries on the Web" (NDSS 2017). Lauinger et al.
- HTTP Archive: [httparchive.org](https://httparchive.org/) — public dataset of web page loads, available via BigQuery
- CDNJS: [cdnjs.com](https://cdnjs.com/) — public library database with all published versions
- Minification Benchmarks: [github.com/privatenumber/minification-benchmarks](https://github.com/privatenumber/minification-benchmarks)
