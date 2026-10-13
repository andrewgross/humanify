# esbuild-minified-esm — minified, as an ES module (split skipped: spec I25)

A real esbuild 0.28.2 build of `../node-app` with
`--bundle --minify --format=esm --platform=node`.

The guarded helpers at the TOP LEVEL of an ES module, with a real top-level
`import`. Detection and the vendor extraction work (finding #96: 26 files in
`vendor/`); the split does not read this layout (plugin-spec I25, the P9
bundle layout), so the run exits 0 with a WARNING and writes the named
output unsplit (`splitMethod: not-split`), as `esbuild-esm` does.

## The build (PINNED: esbuild 0.28.2)

`bash test/e2e/fixtures/node-app/build.sh` (see its README).
sha256 (2026-10-12):

- v1.0.0: `add7b82877c3966d61693fe9cae2df34b37a0970c21fd02c8da9450d542b6ed8`
- v1.1.0: `5b33456198263d05a942d2c6e27d1993c8ac91a68c6bb48f2ab055a702a5f88a`
