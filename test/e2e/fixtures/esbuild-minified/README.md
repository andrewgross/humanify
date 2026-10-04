# esbuild-minified — esbuild's production shape (KNOWN GAP: spec I2)

A real esbuild 0.27.2 build of `../node-app` with
`--bundle --minify --format=iife --platform=node`: one line, every name
shortened — including esbuild's own runtime helpers. `__commonJS` becomes
`var n=(t,e)=>()=>(e||t((e={exports:{}}).exports,e),e.exports)`.

## What it records today (main @ 8883d0b5)

```
humanify detect --toolchain build/v1.0.0/build/index.js
bundler=unknown (tier unknown), minifier=terser (tier unknown),
unpack adapter=passthrough, name profile=bun (fallback), module fossils=none
```

esbuild's detector looks for its helper NAMES (`__commonJS`, `__toESM`,
`__toCommonJS`, `var __export`, `__require`), which `--minify` removes; there
is no banner, so nothing else fires. The minifier verdict is `terser` from
the `!0/!1` coercions (spec I4: it is esbuild's minifier).

The pipeline still RUNS: fresh, prior and `--sequential` (twice each) all
exit 0, are byte-deterministic, and the split tree (9 files, one folder)
boots with the input's exact stdout. But the passthrough adapter extracts
nothing, so all 25 dependencies stay in the app half — no `vendor/`, no
vendor naming skip, no module-layout fossils (fresh grouping).

`--bundler esbuild` on the same builds: 26 extracted files in `vendor/`
and both legs boot identically — the extraction already handles the
minified factory shape. The fix is detection alone.

## The known-gap entry

`scripts/e2e.ts` `KNOWN_GAPS` holds this fixture to its `expect` block
(`bundler: esbuild`, `unpackAdapter: esbuild`, `vendor: true`) failing with
`detection: bundler is unknown, expected esbuild`. When I2 is fixed the
fixture PASSES and the stage fails, telling the fix to delete the entry.

## The build (PINNED: esbuild 0.27.2)

`bash test/e2e/fixtures/node-app/build.sh` (see its README).
sha256 (2026-10-04):

- v1.0.0: `23ab2e93079c88a0d3965af67172dcbb52aef35d8ce28773b9513c67222d78eb`
- v1.1.0: `f863d379d8866aed02a9cd1017114fe7e97ede05db815edda3e4d77fe098f783`
