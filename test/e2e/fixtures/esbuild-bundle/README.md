# esbuild-bundle — the esbuild unpack adapter's e2e fixture

A real [esbuild](https://esbuild.github.io/) bundle: the second bundler the
pipeline reads, added with the esbuild unpack adapter (the TS-era reference
is exp075 / ac56eac0). It exercises the whole esbuild surface in 8KB:

- `__commonJS({...})` — esbuild's CJS factory in OBJECT form: the vendor
  extraction pulls the method's function into `vendor/` (supplying the
  `function` keyword a method span lacks), and the object's key (the
  module's original source path) is recorded on the manifest entry as
  `sourcePath` (never load-bearing).
- `__esm({...})` — esbuild's lazy-ESM init in object form: the fossil
  split reads this grammar, and the unminified build hands the module's
  source path over the same way (the ledger's
  `fossilModules[].sourcePath`).
- an iife wrapper behind esbuild's `"use strict";` directive prologue,
  with enough wrapper-scope bindings (60 after the vendor extraction) to
  clear the frozen wrapper-detection threshold — the split needs it.
- a deterministic stdout report (`normalize: …`, `fibonacci: …`): the e2e's
  bundle boot step proves the runnable split tree behaves exactly like the
  input bundle (`node run.cjs` vs `node <input>` — same stdout, same exit).

## The build (PINNED: esbuild 0.27.2)

The committed `build/v<ver>/build/index.js` files are byte-reproducible.
Regenerate from the repo root (the source paths esbuild bakes into the
object keys are relative to the build's working directory):

```bash
npx esbuild@0.27.2 test/e2e/fixtures/esbuild-bundle/source/v1.0.0/src/main.js \
  --bundle --format=iife \
  --outfile=test/e2e/fixtures/esbuild-bundle/build/v1.0.0/build/index.js
npx esbuild@0.27.2 test/e2e/fixtures/esbuild-bundle/source/v1.1.0/src/main.js \
  --bundle --format=iife \
  --outfile=test/e2e/fixtures/esbuild-bundle/build/v1.1.0/build/index.js
```

Verification history: 0.27.2 (2026-10-02 — the same version the TS-era
reference exp075 verified against).

## Why this fixture needs `"bundle": true` in its config

The single-file fixtures run the pipeline without `--split` (their output
is one rewritten file). A bundle's unpack output is a TREE (`vendor/` +
`runtime.js`), so bundle fixtures run every leg WITH `--split` and boot the
runnable CJS module graph in place; `scripts/e2e.ts` reads the flag. The
legacy-golden comparison is skipped for this pair (no pre-2026-09-28
binary ever ran it — there is no golden to prove anything against), with a
printed note.

The fixture's heads-up for future edits: the wrapper-detection threshold
(frozen at 50 wrapper-scope bindings) is measured on the text WITH the
vendor already extracted — this bundle sits at ~60 after extraction. If a
source edit removes wrapper-scope declarations, the split starts failing
loud (`no recognizable bundle wrapper`); grow the local ESM modules (whose
bodies esbuild scope-hoists) rather than the CJS dep.
