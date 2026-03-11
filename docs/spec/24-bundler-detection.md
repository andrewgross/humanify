# Bundler Detection

## Problem

The pipeline currently runs webcrack unconditionally, which only handles webpack and browserify bundles. Rollup, esbuild, and Bun bundles pass through unchanged with no metadata recovered. Without knowing what produced the input, the pipeline can't make informed decisions about:

- **Unpacking strategy**: Webpack needs module extraction; Rollup/esbuild are scope-hoisted and need structure recovery instead
- **Minified name patterns**: Terser uses sequential single-char mangling, Bun uses 3-char mixed-case with `$`, esbuild uses short lowercase — `looksMinified()` thresholds should vary
- **Library detection**: Layer 1 path matching (spec 05) only works for webpack — Rollup bundles need Layer 3+ detection
- **Structure recovery**: Per-bundler heuristics (spec 25) need to know which bundler produced the input

## Goals

1. Identify the bundler and minifier that produced the input as the first pipeline step
2. Route to appropriate adapters (webcrack for webpack/browserify, passthrough for scope-hoisted bundles)
3. Surface detection results to downstream stages (library detection, renaming, structure recovery)
4. Support manual override via CLI flags for cases where auto-detection fails

## Non-Goals

- Deobfuscation or unpacking (that's the adapter's job)
- Detecting specific library content (that's spec 05)
- Recovering module boundaries (that's spec 25)
- Supporting every bundler ever made — focus on the top 5-6 by market share

## Architecture

### Detection Model

Bundler and minifier are **independent dimensions**. A bundle can be "Rollup + Terser" or "esbuild + esbuild" or "webpack + SWC". Detection returns both with a classification tier:

```typescript
interface DetectionResult {
  bundler?: { type: BundlerType; tier: DetectionTier; version?: string };
  minifier?: { type: MinifierType; tier: DetectionTier };
  signals: DetectionSignal[];
}

type BundlerType = "webpack" | "browserify" | "rollup" | "esbuild" | "parcel" | "bun" | "unknown";
type MinifierType = "terser" | "esbuild" | "swc" | "bun" | "none" | "unknown";
type DetectionTier = "definitive" | "likely" | "unknown";

interface DetectionSignal {
  source: string;         // e.g., "webpack-require", "rollup-deconflict"
  pattern: string;        // The matched pattern or description
  bundler?: BundlerType;
  minifier?: MinifierType;
  tier: DetectionTier;
}
```

### Detection Phases

Detection runs three phases sequentially, short-circuiting when a definitive signal is found:

#### Phase 1: Definitive Signals (string matching, first 16KB)

Fast substring/regex checks for bundler-specific runtime artifacts:

| Signal | Bundler | Pattern |
|--------|---------|---------|
| `__webpack_require__` | webpack | Webpack module loader |
| `__webpack_modules__` | webpack | Module array/object |
| `webpackChunk` | webpack | Chunk loading (v5) |
| `installedModules` + `__webpack_require__` | webpack v4 | Older module cache |
| `__commonJS` + `__toESM` | esbuild | ESM interop helpers |
| `__require` + `__export` | esbuild | CJS/ESM bridge |
| `parcelRequire` | parcel | Parcel module registry |
| `require("_bundle_loader")` | parcel | Parcel v1 |
| `modules[id]` + `installedModules` | browserify | Browserify runtime |

These are unique enough to be definitive (`tier: "definitive"`). A single match is sufficient.

#### Phase 2: Statistical Signals (pattern analysis, first 32KB)

When Phase 1 finds nothing (scope-hoisted bundles), look for structural patterns:

| Signal | Bundler | Pattern |
|--------|---------|---------|
| `$N` deconflicting | rollup | `render$1`, `state$2` suffix pattern |
| `/*#__PURE__*/` annotations | rollup or esbuild | Tree-shaking markers |
| `Object.freeze(Object.defineProperty(..., '__esModule'))` | rollup | ESM interop |
| `Object.defineProperty(exports, '__esModule')` | rollup/esbuild | CJS output |
| Top-level var concatenation | rollup or esbuild | No IIFE wrappers |

Multiple weak signals from the same bundler combine: 2+ signals → `"likely"`, otherwise → `"unknown"`.

#### Phase 3: Minifier Signals (independent of bundler)

| Signal | Minifier | Pattern |
|--------|----------|---------|
| `void 0` for undefined | terser | Terser's default |
| Sequential single-char mangling (`a,b,c,d,e`) | terser | Default alphabet |
| `!0` for true, `!1` for false | terser | Boolean compression |
| 3-char mixed-case with `$` (`aB$`, `xY$`) | bun | Bun's mangling style |
| Short lowercase identifiers (`a,b,c` without sequential pattern) | esbuild | esbuild default |
| `/*#__PURE__*/` density | esbuild | More aggressive annotation |

### Confidence vs Determinism

Rather than arbitrary numeric confidence scores, detection uses a **discrete three-tier classification**:

- **Definitive**: A unique runtime artifact was found. No ambiguity.
- **Likely**: Multiple statistical signals point to the same bundler. High confidence but not certain.
- **Unknown**: Insufficient or conflicting signals. Pipeline uses conservative defaults.

Whether this discrete model suffices or continuous scores are needed is an open question to be resolved experimentally (see Open Questions).

### Adapter System

Each bundler type maps to an adapter that handles the unpacking/preparation step:

```typescript
interface BundlerAdapter {
  name: string;
  supports(detection: DetectionResult): boolean;
  unpack(code: string, outputDir: string): Promise<UnpackResult>;
}

interface UnpackResult {
  files: OutputFile[];
  moduleMetadata?: Map<string, ModuleMetadata>;
}
```

Two initial adapters:

1. **WebcrackAdapter**: Wraps the existing `webcrack()` call. Used for webpack and browserify bundles. Produces per-module files with module metadata (paths, IDs, entry flags).

2. **PassthroughAdapter**: For scope-hoisted bundles (Rollup, esbuild, Bun). Passes the input through unchanged — these bundles need structure recovery (spec 25), not module extraction.

Manual override: `--bundler <type>` and `--minifier <type>` CLI flags bypass auto-detection and force a specific adapter.

### Pipeline Position

Detection replaces the unconditional `webcrack()` call:

```
CURRENT:   webcrack() → detectLibraries() → plugins → output
PROPOSED:  detect() → adapt() → detectLibraries() → plugins → output
```

In `src/unminify.ts`:

```typescript
// Before: unconditional webcrack
const { files } = await webcrack(bundledCode, outputDir);

// After: detect-then-adapt
const detection = await detectBundle(bundledCode);
const adapter = selectAdapter(detection, options);
const { files, moduleMetadata } = await adapter.unpack(bundledCode, outputDir);
```

### Downstream Consumers

The `DetectionResult` flows to:

1. **Library detection** (spec 05): Layer 1 path matching is skipped for non-webpack bundles (no module paths to match). Layer 3 comment regions and Layer 4 fingerprints run regardless.

2. **`looksMinified()`**: Per-minifier identifier pattern thresholds. Terser's sequential mangling is easy to detect; Bun's mixed-case style has different entropy characteristics.

3. **Structure recovery** (spec 25): Uses bundler type to select per-bundler heuristics (Rollup deconflict grouping, webpack module validation).

4. **Rename processor**: Could adjust LLM prompts to include bundler context ("this code was bundled with Rollup" helps the LLM understand deconflict suffixes).

### Detection Result Caching

Detection results should be cached to avoid re-running on repeated invocations with the same input. This depends on the checkpoint DB design (spec 16).

- **Cache key**: Content hash (SHA-256) of the input file
- **Cached fields**: Full `DetectionResult` (bundler, minifier, signals)
- **Invalidation**: Content hash changes → re-detect

The spec defers the specific DB schema to spec 16. Detection should expose a `cacheKey(input: string): string` function that the checkpoint system can use.

## Types

```typescript
// src/detection/types.ts

interface DetectionResult {
  bundler?: { type: BundlerType; tier: DetectionTier; version?: string };
  minifier?: { type: MinifierType; tier: DetectionTier };
  signals: DetectionSignal[];
}

type BundlerType = "webpack" | "browserify" | "rollup" | "esbuild" | "parcel" | "bun" | "unknown";
type MinifierType = "terser" | "esbuild" | "swc" | "bun" | "none" | "unknown";
type DetectionTier = "definitive" | "likely" | "unknown";

interface DetectionSignal {
  source: string;
  pattern: string;
  bundler?: BundlerType;
  minifier?: MinifierType;
  tier: DetectionTier;
}

interface BundlerAdapter {
  name: string;
  supports(detection: DetectionResult): boolean;
  unpack(code: string, outputDir: string): Promise<UnpackResult>;
}

interface UnpackResult {
  files: OutputFile[];
  moduleMetadata?: Map<string, ModuleMetadata>;
}
```

## File Structure

```
src/detection/
  types.ts                 # DetectionResult, BundlerType, MinifierType, etc.
  detect.ts                # Main detectBundle() — runs all phases
  adapters.ts              # BundlerAdapter interface, selectAdapter()
  adapters/
    webcrack.ts            # Wraps existing webcrack plugin
    passthrough.ts         # For scope-hoisted bundles
  signals/
    webpack.ts             # __webpack_require__, webpackChunk, etc.
    browserify.ts          # modules[id] + installedModules patterns
    rollup.ts              # $N deconflicting, Object.freeze interop
    esbuild.ts             # __commonJS, __toESM, __require
    bun.ts                 # Bun-specific patterns
    parcel.ts              # parcelRequire patterns
    minifier.ts            # Minifier-independent signals (void 0, !0, mangling)
```

## Testing Approach

Implementation follows strict Red/Green TDD:

### Unit Tests (per detector)

Each signal detector is a pure function: `(code: string) => DetectionSignal[]`. Tests provide known bundle snippets and assert correct signals.

1. **RED**: Write failing test — webpack snippet → expect `"webpack"` signal with `tier: "definitive"`
2. **GREEN**: Implement `detectWebpackSignals()`
3. Repeat for each bundler/minifier detector

### Confusion Matrix

Build a matrix test that runs **every bundler's output** through **every detector**:

```
              webpack  browserify  rollup  esbuild  bun  parcel
webpack.js      ✓         ✗         ✗       ✗      ✗      ✗
browserify.js   ✗         ✓         ✗       ✗      ✗      ✗
rollup.js       ✗         ✗         ✓       ✗      ✗      ✗
esbuild.js      ✗         ✗         ✗       ✓      ✗      ✗
```

Definitive signals must have **zero cross-contamination**. Statistical signals may overlap (Rollup and esbuild share some patterns) — the test documents where this happens.

### Fixture Generation

Bundle the same source with each bundler to create test fixtures:

1. Choose a small library (e.g., nanoid, zustand) as the source
2. Bundle with: webpack 5, Rollup, esbuild, Bun, Parcel, browserify
3. Minify each with: Terser, esbuild, SWC, Bun
4. This produces a matrix of `6 bundlers × 4 minifiers = 24 fixtures`
5. Run detection on all, record signals observed → confusion matrix

### Integration Tests

1. **Adapter routing**: Detection result → correct adapter selected
2. **CLI override**: `--bundler webpack` → WebcrackAdapter regardless of detection
3. **Unknown input**: Random JS file → `"unknown"` tier, passthrough adapter

## Implementation Phases

### Phase 1: Definitive Signals + Adapter Routing

- [ ] `DetectionResult` and signal types
- [ ] Definitive signal detectors for webpack, browserify, esbuild, parcel
- [ ] `WebcrackAdapter` wrapping existing `webcrack()` call
- [ ] `PassthroughAdapter` for unrecognized/scope-hoisted bundles
- [ ] Wire into `unminify.ts` replacing unconditional `webcrack()`
- [ ] CLI flags: `--bundler`, `--minifier`
- [ ] Unit tests + confusion matrix for definitive signals

### Phase 2: Statistical Signals

- [ ] Rollup `$N` deconflict pattern detector
- [ ] `/*#__PURE__*/` annotation density detector
- [ ] Minifier signal detectors (void 0, !0, mangling alphabet)
- [ ] Signal combination logic (multiple weak → `"likely"`)
- [ ] Extended confusion matrix with statistical signals

### Phase 3: Downstream Integration

- [ ] Pass `DetectionResult` to library detection (spec 05)
- [ ] Per-minifier `looksMinified()` thresholds
- [ ] Pass `DetectionResult` to structure recovery (spec 25)
- [ ] Detection result caching (depends on spec 16 checkpoint DB)

## Experiments

Each open question is resolved through a hypothesis → fixture → experiment → results cycle:

### Experiment 1: Parcel Detection

- **Hypothesis**: Parcel bundles contain `parcelRequire` or similar unique runtime artifacts
- **Method**: Bundle a fixture with Parcel v2, examine first 16KB for unique strings
- **Expected outcome**: At least one definitive signal unique to Parcel

### Experiment 2: Webpack v4 vs v5 Disambiguation

- **Hypothesis**: Webpack v4 and v5 have distinguishable runtime patterns
- **Method**: Bundle same source with both, diff signal sets
- **Decision**: If reliably distinguishable with low complexity, add version detection; otherwise treat as single `"webpack"` type

### Experiment 3: Bun vs esbuild Disambiguation

- **Hypothesis**: Bun and esbuild outputs are reliably distinguishable beyond `__require`
- **Method**: Bundle same source with both, find signals unique to each
- **Decision**: If ambiguous, document the overlap and accept `"likely"` tier for these

### Experiment 4: Discrete vs Continuous Classification

- **Hypothesis**: A three-tier discrete classification (definitive/likely/unknown) suffices for all practical inputs
- **Method**: Run detection on all existing fixtures, record all signals, identify any genuinely ambiguous cases
- **Decision**: If ambiguous cases exist where continuous scores would help routing, add them; otherwise keep discrete

### Experiment 5: Performance on Large Files

- **Hypothesis**: Detection on the first 16-32KB is fast enough (<50ms) even for 10MB+ inputs
- **Method**: Benchmark detection on synthetic large files
- **Decision**: If slow, consider streaming or sampling strategies

## Open Questions

1. **Parcel detection** → Experiment 1
2. **Webpack v4 vs v5** → Experiment 2
3. **Bun vs esbuild overlap** → Experiment 3
4. **Discrete vs continuous classification** → Experiment 4
5. **Performance on large files** → Experiment 5
6. **Detection caching**: Results cached in checkpoint DB (spec 16 dependency). Cache key is content hash of input. Deferred until spec 16 DB schema is finalized.
7. **SWC minifier detection**: SWC is increasingly popular but its output patterns are not well-documented. May need a dedicated experiment.
