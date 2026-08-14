# The Bun fossil grammar — validated against a known-source corpus

Corpus: `corpus/src/` (18 files: nested folders; default/named/namespace/
re-export imports; side-effectful, constants-only, hoisted-functions-only,
CJS-interop, cyclic, and dynamically-imported modules). Bundled with Bun
1.3.14, `--target=bun --format=cjs`, minify on/off — the real Claude Code
banner (`// @bun @bytecode @bun-cjs` + CJS wrapper) matches this mode.
Outputs in `corpus/out/`; every rule below was read off a bundle whose
source layout we control (the answer key).

## Rule 1 — eager modules leave NO per-module fossil (confidence: HIGH)

A module reachable only through static imports is FLATTENED: its hoisted
functions and initialized `var`s are emitted inline, in topological
order, separated only by `// src/path.ts` comments — which minification
deletes. A fully-static bundle has no init functions at all
(`out/bun-cjs-min.js` is one undifferentiated statement stream).
**Corollary: the real bundle's thousands of init functions mean most of
Claude Code's app graph is reachable from laziness boundaries.**

## Rule 2 — the lazy-module segment shape (confidence: HIGH)

A module that must be forceable on demand (dynamic-import target, or any
module transitively imported by one) compiles to a CONTIGUOUS segment:

```js
// src/lazy/feature.ts                     <- dies under --minify
var exports_feature = {};                  // namespace obj (only if ns-imported)
__export(exports_feature, {                // exported-name map: name -> local
  featureMain: () => featureMain
});
function featureMain() { ... }             // hoisted decls, OUTSIDE the init
var entries;                               // hoisted var decls, uninitialized
var init_feature = __esm(() => {           // THE FOSSIL
  init_registry();                         // leading init calls = IMPORTS
  entries = new Map;                       // write-set = module's own values
});
```

- **Write-set → membership**: every assignment target in the init body is
  a top-level var belonging to that module.
- **Leading `init_*()` calls → the import graph**, one edge per imported
  lazy module (eager imports leave no call — they are already evaluated).
- **`__export` map → exported names**, mapping public name → local
  binding (function decls attributable even though they sit outside).
- The segment is emitted contiguously; the init def is its terminator.

## Rule 3 — what survives minification (confidence: HIGH)

Comments die; the STRUCTURE survives verbatim: `__esm` wrappers, write
sets, leading init calls, `__export` maps, `__toESM`/`__commonJS`
interop, and `Promise.resolve().then(() => (init_X(), exports_X))` for
dynamic imports (the import edge survives as two identifiers). Local
names are minified but the humanify pipeline re-names them — in real
humanified bundles the inits are the `initializeApp*` family.

## Rule 4 — CJS modules use the factory pattern (confidence: HIGH)

`.cjs` sources become `var require_old = __commonJS((exports, module) =>
{...})` — the vendor-factory pattern the pipeline already sets aside —
and ESM importers access them through `__toESM(require_old(), 1)` (the
forwarding-stub shape exp062 dissected).

## Rule 5 — what the fossil CANNOT see (rule-8 enumeration)

1. **Eager modules** (rule 1): no init, no segment marker after
   minification. Attribution only by topological contiguity between
   lazy segments — weak evidence, order-stable but boundary-blind.
2. **Function-only cycles** flatten eagerly (the corpus cycle produced
   no init) — cycles alone do not force laziness.
3. **Merged single-use inits**: none observed in the corpus, but
   minified inlining of a single-call init is plausible at scale;
   detect by init-count vs export-obj-count divergence.
4. **Type-only modules** vanish entirely (corpus `types.ts` left no
   trace) — correct, nothing to attribute.
5. The `// src/path.ts` comments carry the ORIGINAL FILE PATHS —
   available only if we ever ingest an unminified build; the shipped
   bundle is minified, so paths must come from naming, not fossils.
