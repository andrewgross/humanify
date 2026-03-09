# Experiment 004 Results: File Emission

## Metrics

| Metric | Exp 003 (dirty fixture) | Exp 004 (clean fixture) |
|--------|------------------------|------------------------|
| Input lines | 2762 | 1664 |
| Top-level functions | 95 | 61 |
| Clusters | 3 | 2 |
| Shared % | 0% | 0% |
| MQ score | 1.000 | 1.000 |
| Output files | 6 | 5 |

## Output file summary

| File | Lines | Purpose |
|------|-------|---------|
| `hydrate_cloneElement_createContext.js` | 1231 | Core: vdom, diff, render, DOM, component lifecycle |
| `useEffect_useRef_useImperativeHandle.js` | 367 | Hooks: useState, useEffect, etc. + options patching |
| `shared.js` | 40 | Constants, mutable state vars, hooks init wiring |
| `orphans.js` | 17 | `toChildArray` (orphan utility) |
| `index.js` | 3 | Barrel re-exports |

## Fixes applied

### Fix 1: Clean fixture — SUCCESS
Removed 1098 lines of duplicated code. The "third cluster" from exp 003 was entirely the duplicate. Now we have 2 real clusters: core (30 functions) + hooks (30 functions), plus 1 orphan.

### Fix 2: Export only referenced names — SUCCESS
Core file exports dropped from 32 to 10. Only names actually imported by other files or in the barrel export are exported.

### Fix 3: shared.js as leaf dependency — PARTIAL
shared.js is much smaller (40 lines vs 86) and only contains constants + state variables. However, it **still imports `options$1` from core** because `const options = options$1` was placed in shared.js. This creates a dependency inversion (shared depends on core).

## Quality checklist

- [x] Functions that work together end up in the same file — core functions together, hooks together
- [ ] File sizes are roughly balanced — core is 1231 lines vs hooks 367 (3.4:1 ratio)
- [x] Each file has a coherent theme/purpose — yes
- [ ] No circular imports — core→hooks (`doRender`), shared→core (`options$1`)
- [ ] shared.js is minimal and has no imports — minimal but still has 1 import
- [x] A developer new to this codebase would understand the organization

## Remaining issues

### 1. Circular import: core → hooks via `doRender`
`doRender` is a 1-line function (`return this.constructor(vnode, context)`) used as a fallback render method for function components in `diff()`. It was placed in hooks because of proximity or edge weight, but semantically it belongs in core. The clustering algorithm groups it with hooks because it was defined near the hooks code in the original source.

### 2. shared.js imports from core
`const options = options$1` landed in shared.js because it references `options$1` (defined in core). But `options` is then used by hooks. The assignment chain is: core defines `options$1` → shared creates `const options = options$1` → hooks uses `options`. This should be simplified.

### 3. File size imbalance
Core is 75% of the code (1231/1658 lines). This matches the original Preact structure (core is much larger than hooks) so this is inherent, not a splitting problem.

## Next steps

1. **Fix `doRender` placement**: Either force it into core during ledger assignment (if its only caller is in core) or add a post-processing step that breaks circular imports by moving functions to their primary consumer's file.
2. **Fix shared.js imports**: Move `const options = options$1` and the `oldBeforeDiff`/`oldBeforeRender` etc. wiring into the hooks file (since hooks is the only consumer of these variables).
3. **Consider renaming**: File names like `hydrate_cloneElement_createContext.js` are not ideal. This is deferred to LLM naming.
