# Experiment 004 Results: File Emission

## Metrics

| Metric | Exp 003 (dirty fixture) | Exp 004a (clean, no cycle fix) | Exp 004b (with cycle fix) |
|--------|------------------------|-------------------------------|--------------------------|
| Input lines | 2762 | 1664 | 1664 |
| Top-level functions | 95 | 61 | 61 |
| Clusters | 3 | 2 | 2 |
| Shared % | 0% | 0% | 0% |
| MQ score | 1.000 | 1.000 | 1.000 |
| Output files | 6 | 5 | 5 |
| Circular imports | ? | 2 (core↔hooks, shared→core) | 0 |
| shared.js imports | ? | 1 (options$1 from core) | 0 |

## Output file summary (004b — final)

| File | Lines | Purpose |
|------|-------|---------|
| `hydrate_cloneElement_createContext.js` | 1234 | Core: vdom, diff, render, DOM, component lifecycle, doRender |
| `useEffect_useRef_useImperativeHandle.js` | 373 | Hooks: useState, useEffect, etc. + options aliasing + options patching |
| `shared.js` | 32 | Constants + mutable state vars (pure leaf, no imports) |
| `orphans.js` | 18 | `toChildArray` (orphan utility) |
| `index.js` | 4 | Barrel re-exports |

## Import graph (004b — final, DAG)

```
core → shared (constants: EMPTY_ARR, EMPTY_OBJ, etc.)
hooks → core (options$1)
hooks → shared (state vars: currentComponent, currentHook, etc.)
orphans → shared (isArray)
index → core, hooks, orphans (barrel re-exports)
```

No circular imports. shared.js is a pure leaf dependency.

## Fixes applied

### Fix 1: Clean fixture — SUCCESS
Removed 1098 lines of duplicated code. The "third cluster" from exp 003 was entirely the duplicate. Now we have 2 real clusters: core (30 functions) + hooks (30 functions), plus 1 orphan.

### Fix 2: Export only referenced names — SUCCESS
Core file exports dropped from 32 to 10. Only names actually imported by other files or in the barrel export are exported.

### Fix 3: shared.js as leaf dependency — SUCCESS
Added `resolveImportCycles()` post-processing step that:
1. **Iteratively cleans shared.js**: Moves entries with external dependencies to their primary consumer. Iteration handles cascading deps (moving `options` causes `oldBeforeDiff` etc. to also need moving).
2. **Breaks 2-file cycles**: Detects A↔B cycles and moves the declaring entries for the minority import direction.

Result: `const options = options$1` and `oldBeforeDiff`/`oldBeforeRender`/etc. moved from shared→hooks. `doRender` moved from hooks→core. shared.js went from 40 lines with 1 import to 32 lines with 0 imports.

## Quality checklist

- [x] Functions that work together end up in the same file — core functions together, hooks together
- [ ] File sizes are roughly balanced — core is 1234 lines vs hooks 373 (3.3:1 ratio)
- [x] Each file has a coherent theme/purpose — yes
- [x] No circular imports — clean DAG
- [x] shared.js is minimal and has no imports — 32 lines, pure constants + state vars
- [x] A developer new to this codebase would understand the organization

## Remaining issues

### 1. File size imbalance
Core is 74% of the code (1234/1661 lines). This matches the original Preact structure (core is much larger than hooks) so this is inherent, not a splitting problem.

### 2. File naming
Names like `hydrate_cloneElement_createContext.js` are not ideal. This is deferred to LLM naming in a future experiment.

### 3. Hook-specific state vars in shared.js
Variables like `currentComponent`, `currentHook`, `afterPaintEffects`, `previousComponent` are only used by hooks. They're in shared.js because they don't have external deps (they're just `let` declarations). A future optimization could move vars that are only imported by a single file into that file.

## Next steps

1. **Move single-consumer shared vars**: Variables in shared.js that are only imported by one file should be moved to that file. This would slim shared.js down to just truly shared constants.
2. **LLM file naming**: Use the LLM to generate meaningful file names instead of `hydrate_cloneElement_createContext.js`.
3. **Test with another fixture**: Try the split pipeline on a different bundled library to validate generalization.
