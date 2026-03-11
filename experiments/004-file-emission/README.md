# Experiment 004: File Emission

## Background

Experiment 003 achieved clean clustering (3 clusters, 0% shared, MQ 1.0) but only output a manifest. This experiment adds file emission and runs the first qualitative review.

### Fixture fix

The original fixture (`001-baseline-clustering/fixtures/preact-v1/output/deobfuscated.js`) contained **duplicate code**: the Rollup scope-hoisted bundle included both the unminified preact core AND a minified copy (bundled inline for hooks). Humanify renamed both copies independently, producing ~1100 lines of duplicate functions with different names. This inflated function count from ~51 to ~95 and created a phantom second cluster.

The clean fixture (`fixtures/preact-clean.js`) removes lines 1264-2364 of the deobfuscated output (the humanified duplicate) and wires hooks to use the unminified core's `options$1` object directly.

## Hypothesis

With the clean fixture and file emission:
1. Clustering should produce 2-3 meaningful files (core/DOM + hooks, possibly separated)
2. Emitted files should have correct imports/exports with no circular dependencies
3. `shared.js` should contain only true shared constants, not hooks state or init code

## Variables

- Input: `fixtures/preact-clean.js` (1664 lines, 51 functions)
- `--min-cluster-size 3 --proximity`
- Emitter: hybrid source-range + Babel imports/exports

## Fixes applied in this experiment

### Fix 1: Clean fixture (high severity)
Remove duplicate code from input. See "Fixture fix" above.

### Fix 2: Export only referenced names (medium severity)
Previous emission exported ALL declared names from every file. Now only export names that are actually imported by other files (or appear in the barrel export).

### Fix 3: shared.js should be a leaf dependency (high severity)
Previous emission allowed shared.js to import from other files, inverting the dependency direction. Evaluate whether clean fixture resolves this naturally.

## Steps

```bash
npx tsx src/index.ts split experiments/004-file-emission/fixtures/preact-clean.js \
  --min-cluster-size 3 --proximity \
  --output experiments/004-file-emission/output/
```

## Quality checklist

- [ ] Functions that work together end up in the same file
- [ ] File sizes are roughly balanced
- [ ] Each file has a coherent theme/purpose
- [ ] No circular imports
- [ ] shared.js is minimal and has no imports
- [ ] A developer new to this codebase would understand the organization
