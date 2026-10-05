# bun-bundle — a real, unminified Bun build

A real [Bun](https://bun.sh/) bundle in **Claude Code's own layout**:
`bun build --target=bun --format=cjs` writes the `// @bun @bun-cjs` banner
and wraps the whole program in one CommonJS wrapper function,
`(function(exports, require, module, __filename, __dirname) {…})`. Until
this fixture the gate had no real Bun build at all (toolchain review H4):
the only Bun input the pipeline saw was Claude Code itself, which is
minified.

## Why it exists: detection by strength (review R3)

Bun's runtime helpers carry **esbuild's names** — `__commonJS`, `__toESM`,
`__export`, `__require` (Bun copied esbuild's runtime). An unminified Bun
build therefore fires BOTH bundlers' signals: the `// @bun` banner and
five esbuild helper names. Detection used to take the first definitive
signal in detector order, and esbuild's detector runs before Bun's, so this
build was detected as **esbuild** (the esbuild vendor stamp, the esbuild
name profile, naming groups of 15). Detection now ranks signals by
strength — a banner the bundler wrote about itself beats a helper name two
bundlers share — and this build detects as **Bun**, with the conflict
written into the verdict:

```
humanify detect test/e2e/fixtures/bun-bundle/build/v1.0.0/build/index.js
{"bundler":{"type":"bun","tier":"definitive","conflict":{"resolution":"strength",
  "candidates":[{"bundler":"bun","pattern":"// @bun banner","strength":"banner"},
                {"bundler":"esbuild","pattern":"__commonJS","strength":"shared-helper-name"}]}}, …}
```

`detect_test::a_real_unminified_bun_build_is_detected_as_bun` reads both
committed builds; on main @ 342650cf it failed (`esbuild`).

## What the builds hold

- 24 local CommonJS deps (`src/vendor/modNN.cjs`, shared with
  `esbuild-bundle-small`) and the real npm package **ms@2.1.3** — all
  `__commonJS` factories, extracted to `vendor/`;
- an ES module (`src/late.js`) reached only through `require()` from a
  CommonJS module (`src/legacy.cjs`), with a top-level side effect — so Bun
  wraps it in a lazy `__esm` init (`init_late`). It was planted so the
  module-marker ("fossil") split, then chosen for every Bun bundle, would
  run at all (without one it refused: "no \_\_esm init definitions").
  Since finding #87 (2026-10-05) the split method comes from what the
  bundle contains: this one module covers ~45% of the app code, under the
  99% threshold, so the bundle takes the fresh grouping (with the lazy
  module's end kept as a file boundary). `bun-lazy` holds the marker
  method now;
- a deterministic 29/30-line stdout report. Node runs Bun's CJS output as
  a bare function expression and calls nothing, so the e2e boot step runs
  the input the way Bun's loader does (`BUN_CJS_LOADER` in
  `scripts/e2e.ts`: evaluate the file to the wrapper, call it with a real
  CommonJS module) and compares that with `node run.cjs` in the split tree.

v1.1.0 perturbs the last dep (adds `tripled`), the late module's output
(zero-padded) and the lazy stamp.

## The build (PINNED: Bun 1.3.14)

Byte-reproducible (built twice, identical sha256). `node_modules/` is not
committed; `package.json` pins `ms` exactly. For each version, from a
scratch copy of `source/v<ver>/`:

```bash
cp -r test/e2e/fixtures/bun-bundle/source/v1.0.0 /tmp/bun-v1.0.0
cd /tmp/bun-v1.0.0
npm install --no-audit --no-fund --ignore-scripts
bun build src/main.js --target=bun --format=cjs \
  --outfile=<repo>/test/e2e/fixtures/bun-bundle/build/v1.0.0/build/index.js
```

(and the same for `v1.1.0`). The `// src/…` and `// node_modules/ms/…`
comments Bun writes are relative to the build's working directory, so
build from inside the version's directory.

sha256 (Bun 1.3.14, 2026-10-04):

- v1.0.0: `b906d5b65978fb00ba519fe1a6fb0ee3c426da5d091e8bc949f467e0c6bc6c28`
- v1.1.0: `40727eaf82279561be5d87b9d5a4bab26bbf4d5e583f7f5ee66cbe46a18f51e0`
