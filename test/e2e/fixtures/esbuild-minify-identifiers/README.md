# esbuild-minify-identifiers — renamed helpers, spaced marker

A real esbuild 0.28.2 build of `../node-app` with
`--bundle --minify-identifiers --format=iife --platform=node`.

`--minify-identifiers` alone renames every helper but keeps whitespace and
syntax, so the CommonJS helper is a multi-line
`var g = (n, e) => () => { try { return e || n((e = { exports: {} }).exports, e), e.exports; } … }`.
Neither the tight `{exports:{}}` marker scan nor the `__commonJS` name finds
it; the helper SHAPE does (finding #96). The e2e holds it to: bundler and
adapter esbuild, 26 files in `vendor/`, the fresh grouping, deterministic
legs, a split tree that boots with the input's stdout.

## The build (PINNED: esbuild 0.28.2)

`bash test/e2e/fixtures/node-app/build.sh` (see its README).
sha256 (2026-10-12):

- v1.0.0: `56a1e14bf2f4a961911e24a106635486222f3157a9a7a45774080fae56126349`
- v1.1.0: `90d9795599d494a2cef6772ee746c41524dbd9f7834f57d22d0ffccf571c6fd3`
