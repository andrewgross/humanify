# Webcrack Source Map Migration

## Problem

The humanify pipeline generates source maps that map **deobfuscated → humanified** code, but cannot map back to the **original minified input**. Webcrack sits in the middle of the pipeline and produces no source maps, breaking the chain.

The full source map chain should be:

```
original source → minified JS (+ optional .map) → deobfuscated JS → humanified JS
```

But today:

```
original source → minified JS (+ optional .map) → [WEBCRACK BREAKS CHAIN] → deobfuscated JS → humanified JS
                                                                             ↑                    ↑
                                                                      no source map      source-map-writer.ts
```

## What Webcrack Does Internally

Webcrack (v2.15.1) is a JavaScript deobfuscation tool that performs four major phases:

### Phase 1: Unpack
Detects and extracts modules from webpack/browserify bundles. This phase is **largely position-preserving** — it extracts existing code from wrapper functions without transforming it.

### Phase 2: Deobfuscate
Applies transforms that reverse deliberate obfuscation:
- **String array decoding** — replaces `_0x1234(42)` with the actual string literal. **Destructive**: the call expression is replaced with a string literal of different length.
- **Control flow flattening** — unwinds `switch`-based dispatcher patterns back to sequential code. **Highly destructive**: completely restructures the AST.
- **Dead code removal** — removes unreachable branches. **Destructive**: deletes nodes.
- **Object folding** — inlines computed property accesses. **Mildly destructive**.

### Phase 3: Unminify
Readability transforms:
- Flip negated conditions (`!a ? b : c` → `a ? c : b`)
- Expand comma expressions to statements
- Convert `void 0` to `undefined`
- Rename `_0x`-prefixed variables (simple sequential renaming)

These are **partially position-preserving** — they rearrange nodes but each output node has a clear origin in the input.

### Phase 4: Transpile
Decompiles transpiled patterns back to modern syntax:
- CommonJS → ESM imports/exports
- `React.createElement` → JSX

**Destructive**: completely changes node structure (e.g., call expression → JSX element).

## Which Transforms Are Position-Preserving?

| Transform | Position-preserving? | Notes |
|-----------|---------------------|-------|
| Bundle unpacking | Yes | Extracts existing code from wrappers |
| String array decoding | No | Replaces calls with literals |
| Control flow unflattening | No | Complete AST restructuring |
| Dead code removal | No | Deletes nodes |
| Object folding | Partial | Inlines computed accesses |
| Condition flipping | Partial | Swaps branches, origin is traceable |
| Comma expansion | Partial | One expression → multiple statements |
| `void 0` → `undefined` | Partial | Simple replacement, traceable |
| Variable renaming | Partial | Same positions, different names |
| CJS → ESM | No | Call expressions → import declarations |
| createElement → JSX | No | Call expressions → JSX elements |

**Key insight**: The unpacking phase (Phase 1) is almost entirely position-preserving. The deobfuscation and transpilation phases are where the chain breaks.

## Migration Strategies

### Strategy A: Fork Webcrack, Add Source Map Tracking

Fork webcrack and add Babel's `inputSourceMap`/`sourceMaps` tracking to each transform plugin.

**Pros**:
- Full fidelity — every transform is tracked
- Webcrack's transform quality is already proven
- Minimal user-facing changes

**Cons**:
- Webcrack has ~20+ transform plugins; adding source map support to each is substantial
- Ongoing maintenance burden to keep fork in sync with upstream
- Some transforms (control flow unflattening) have genuinely ambiguous position mappings
- Estimated effort: 3-6 months for a single developer

### Strategy B: Replace Unpacking, Keep Webcrack Optional

Build a custom module extractor that handles webpack/browserify unpacking with full source map support. Keep webcrack as an optional deobfuscation step (accepting the chain break when used).

**Pros**:
- Unpacking is the most position-preserving phase — easiest to add source maps to
- Custom extractor can be tailored to humanify's needs
- Webcrack's deobfuscation is still available when source maps aren't needed
- Modular: users choose fidelity vs. deobfuscation quality

**Cons**:
- Must handle webpack and browserify bundle formats ourselves
- Missing deobfuscation when source maps are enabled
- Two code paths to maintain

### Strategy C: "Unpack Only" Mode for Source Maps

Build a minimal unpacking layer that preserves positions. When `--source-map` is enabled, skip webcrack entirely and only unpack modules. When source maps aren't needed, use webcrack for full deobfuscation.

**Pros**:
- Smallest scope — only need to handle bundle unpacking
- Clean separation: source-map mode gets position fidelity, non-source-map mode gets full deobfuscation
- No fork maintenance

**Cons**:
- Source-map mode output is less readable (no deobfuscation)
- Users must choose between source maps and deobfuscation
- Still need to handle webpack/browserify formats

## Recommended Phased Approach

### Phase 1: Custom Module Extractor (Strategy C)

Build a position-preserving webpack/browserify module extractor. When `--source-map` is enabled, use this instead of webcrack.

1. Parse the bundle AST with Babel (already a dependency)
2. Detect bundle format (webpack bootstrap pattern, browserify `require` map)
3. Extract module bodies as-is, recording their positions in the original bundle
4. Generate source maps from original bundle → extracted modules using `source-map` npm package
5. Chain with existing `source-map-writer.ts` output using `SourceMapGenerator.applySourceMap()`

This gives us: `minified bundle → extracted modules (with .map) → humanified modules (with .map)`

If the user also provides the original `.map` file, we can chain all three: `original source → minified → extracted → humanified`

### Phase 2: Selective Deobfuscation (Strategy B elements)

Identify which of webcrack's unminify transforms (Phase 3) can be reimplemented with source map tracking. These are the "partial" transforms from the table above:
- Condition flipping
- Comma expansion
- `void 0` → `undefined`
- Variable renaming

These are individually small and could be Babel plugins with standard source map support.

### Phase 3: Full Deobfuscation Tracking (if needed)

If demand exists, fork webcrack or reimplement the destructive transforms with best-effort source map tracking. The control flow unflattening and string array decoding are the hardest — each decoded string could map to the string array definition site, but the mapping is inherently lossy.

## Integration with Existing Code

### `source-map-writer.ts`

The current writer captures a single source map from the rename plugin and writes it to disk. To support chaining:

```typescript
// Compose extraction map + rename map
const composed = await new SourceMapConsumer(renameMap);
composed.applySourceMap(await new SourceMapConsumer(extractionMap));
```

The `source-map` npm package (already used by Babel) supports this composition via `SourceMapConsumer.applySourceMap()`.

### CLI Changes

The `--source-map` flag already exists. When enabled:
- Phase 1: Use custom extractor instead of webcrack
- Generate extraction source maps per-module
- Chain extraction maps with rename maps before writing

No new CLI flags needed — the behavior change is internal to the pipeline.

## Open Questions

1. **How common is obfuscated input?** If most users process webpack bundles that are minified but not obfuscated, Strategy C covers the majority of use cases without needing deobfuscation.

2. **Webcrack's module path resolution**: Webcrack resolves module paths using heuristics (require calls, import patterns). Our custom extractor would need to replicate this or accept numeric module IDs as filenames.

3. **Bundle format coverage**: Webpack and browserify cover the vast majority of bundles, but Rollup, Parcel, and esbuild bundles may need additional extractors.
