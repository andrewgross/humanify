# esbuild-cjs — a top-level CommonJS bundle (KNOWN GAP: spec I25)

A real esbuild 0.27.2 build of `../node-app` with
`--bundle --format=cjs --platform=node`, unminified. This is the shape real
Node CLIs publish to npm — `pnpm` (`dist/pnpm.cjs`, 7.7 MB) and `wrangler`
(`wrangler-dist/cli.js`, 14.8 MB) are both esbuild `--format=cjs` builds —
so it is the shape a fifth, non-Claude-Code eval pair would have.

esbuild's runtime helpers (`__commonJS`, `__esm`, `__toESM`, …) and every
module sit at the TOP LEVEL of the file. Node wraps a CommonJS file in its
own module function when it loads it, so there is no wrapper in the TEXT —
unlike Bun's `--format=cjs` (`bun-bundle`), which writes its wrapper out.

## What it records today (main @ 8883d0b5)

```
humanify detect --toolchain build/v1.0.0/build/index.js
bundler=esbuild (definitive), unpack adapter=esbuild,
bundle layout=single-wrapper-function
```

Detection is right. The fresh `--split` run unpacks (26 vendor files
written) and names, then exits 1 exactly as the ES-module builds do:

```
Error: stable split failed before any tree was written: the run's input
bundle has no recognizable bundle wrapper (the frozen ≥50-binding gate is
measured on the input)
```

Spec I25 names ES-module builds; this is the same gap for CommonJS: the
split's one layout needs the wrapper function written into the file. A
layout fix should treat "top-level module, implicit wrapper" as one case
for both formats (the P9 `BundleLayout` slot). Without `--split` the run
exits 0.

## The known-gap entry

`scripts/e2e.ts` `KNOWN_GAPS` holds this fixture to failing with
`the run's input bundle has no recognizable bundle wrapper`; once it
passes, the stage tells the fix to delete the entry.

## The build (PINNED: esbuild 0.27.2)

`bash test/e2e/fixtures/node-app/build.sh` (see its README).
`build/v<ver>/build/package.json` is `"type": "commonjs"`. sha256 (2026-10-04):

- v1.0.0: `4b4d330f1a05b4bb916a88409219cdd6c4a981a5bc65eecc768b1112957a1428`
- v1.1.0: `ee93abbfa562f1f4aacec6f5081c13ac856281847eb7c535c29d5dc65924065e`
