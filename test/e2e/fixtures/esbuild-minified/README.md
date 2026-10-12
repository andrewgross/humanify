# esbuild-minified — esbuild's production shape (spec I2, fixed)

A real esbuild 0.27.2 build of `../node-app` with
`--bundle --minify --format=iife --platform=node`: one line, every name
shortened — including esbuild's own runtime helpers. `__commonJS` becomes
`var n=(t,e)=>()=>(e||t((e={exports:{}}).exports,e),e.exports)`.

## What it records

```
humanify detect --toolchain build/v1.0.0/build/index.js
bundler=esbuild (definitive), unpack adapter=esbuild
```

esbuild's detector used to read only its helper NAMES (`__commonJS`,
`__toESM`, …), which `--minify` removes, so this build detected as unknown
and the passthrough adapter extracted nothing — a declared KNOWN GAP in
`scripts/e2e.ts` until 2026-10-12. Since finding #96 the module helper is
read by its SHAPE (`modules::helper_shape`). Its plain form here is one Bun
writes too (a `shared-shape` Bun signal); esbuild's own `__export`
registrar beside it (`{get:e[r],enumerable:!0}` — Bun's adds
`configurable`) makes it an esbuild `shape` signal, which outranks it.

The e2e holds the fixture to its `expect` block: bundler and adapter
esbuild, 26 files in `vendor/`, the fresh grouping, fresh + prior +
`--sequential` (twice each) deterministic, and the split tree booting with
the input's exact stdout.

## The build (PINNED: esbuild 0.27.2)

`bash test/e2e/fixtures/node-app/build.sh` (see its README).
sha256 (2026-10-04):

- v1.0.0: `23ab2e93079c88a0d3965af67172dcbb52aef35d8ce28773b9513c67222d78eb`
- v1.1.0: `f863d379d8866aed02a9cd1017114fe7e97ede05db815edda3e4d77fe098f783`
