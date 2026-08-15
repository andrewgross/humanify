# 075 — read esbuild's module form (second bundler)

> **STATUS (2026-08-15): BUILT AND GREEN** on branch
> `exp075-esbuild-reader` (`ac56eac`, based on the exp074 stack). Cannot
> merge to main independently — it edits `fossil-map.ts`, which arrives
> with the unmerged exp070 relayout.

## What differs between the two bundlers

Verified against real esbuild 0.27.2 output, not inferred:

| | bun | esbuild |
| --- | --- | --- |
| helper thunk | `(fn, res) => () => (…, ident)` | `(fn, res) => function __init() { return (…), ident; }` |
| a lazy module | `__esm(() => { … })` | `__esm({ "src/utils/format.js"() { … } })` |
| minified | reader already worked (exp072: 154/154) | reader already worked (exp072: 154/154) |

So the whole gap was the UNMINIFIED esbuild shape: an object with one
keyed method, plus a function-expression thunk. ~20 lines of reader
change, +3 tests, verified end-to-end on a real esbuild bundle (the one
lazy module found, its source path recovered exactly — the other modules
were inlined, which is correct: esbuild only defers what must be
deferred, same as bun).

## The gift

**esbuild's object key IS the original source path.** Exposed as
`FossilModule.sourcePath`, optional and absent for bun and for every
minified build — so nothing may depend on it, but where it exists the
file layout needs no inference at all.

## Bundler coverage after this

| bundler | boundaries recoverable? |
| --- | --- |
| bun (minified or not) | yes |
| esbuild (minified or not) | yes |
| rollup | **no** — boundaries erased entirely (exp072); an honest "cannot support", not a bug |
| webpack | untested; its numbered module map is explicit and likely easier than either |
