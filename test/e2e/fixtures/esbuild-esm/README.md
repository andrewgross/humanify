# esbuild-esm — an ES-module bundle (KNOWN GAP: spec I25)

A real esbuild 0.27.2 build of `../node-app` with
`--bundle --format=esm --platform=node`, unminified (so this fixture
isolates the LAYOUT gap from the minified-detection one). Same app as
`esbuild-minified`; the bundle's statements sit at the TOP LEVEL, the
`node:path` builtin stays a real `import { basename } from "node:path"`,
and there is no wrapper function anywhere.

## What it records today (main @ 8883d0b5)

```
humanify detect --toolchain build/v1.0.0/build/index.js
bundler=esbuild (definitive), minifier=terser (tier unknown),
unpack adapter=esbuild, bundle layout=single-wrapper-function
```

Detection is right (the helper names are intact). The minifier verdict
`terser` is wrong — the build is not minified (spec I4: the `void 0` signal fires on any bundle).

The fresh `--split` run unpacks (26 vendor files written) and names, then
exits 1:

```
Error: stable split failed before any tree was written: the run's input
bundle has no recognizable bundle wrapper (the frozen ≥50-binding gate is
measured on the input)
```

The split knows one layout — the whole program inside one wrapper function
with ≥50 names (Bun's CJS wrapper, esbuild's iife) — and an ES-module
bundle has none (spec I25; the P9 `BundleLayout` slot has only that
implementation). With no fresh tree there is no prior, so the v2 leg never
runs. Without `--split` the run exits 0 (naming works).

The error is the ACCURATE reason. Review R10 predicted a misleading
"oxc failed to parse the input bundle" (seven parse sites force script
mode); on this input the wrapper gate answers first. If a layout fix
reaches those parse sites, this fixture will fail differently and the
known-gap entry will say "changed".

## The known-gap entry

`scripts/e2e.ts` `KNOWN_GAPS` holds this fixture to failing with
`the run's input bundle has no recognizable bundle wrapper`. When ESM
layout support lands, it must pass the same checks as the others —
fresh + prior + `--sequential` twice each, byte-deterministic, the split
tree booting with the input's stdout, `vendor/` non-empty — and the stage
then tells the fix to delete the entry.

## The build (PINNED: esbuild 0.27.2)

`bash test/e2e/fixtures/node-app/build.sh` (see its README).
`build/v<ver>/build/package.json` is `"type": "module"`. sha256 (2026-10-04):

- v1.0.0: `2f80d821ab0de4ee99fd7dc7f18bd8731d0b5b94c0667370f878631bc939ea9a`
- v1.1.0: `11b60fb74bf10c8b19e48c76580b73b963d50c542053becda018ad69f7a31e0f`
