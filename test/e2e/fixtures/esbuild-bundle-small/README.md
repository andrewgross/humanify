# esbuild-bundle-small — the under-the-wrapper-threshold boundary fixture

A real [esbuild](https://esbuild.github.io/) bundle whose VENDOR HALF
DOMINATES: the shape that made the split's "is this really a bundled app?"
gate fail legitimate mid-size apps (2026-10-02).

The gate (WP1.5, frozen at ≥50 wrapper-scope bindings) used to measure the
text the split is HANDED — the POST-EXTRACTION runtime, after the vendor
extraction has spliced one wrapper-scope `var require_* = __commonJS({...})`
declaration out per vendored CJS module. A bundle of many small CJS deps
with a modest app half therefore lost exactly the bindings that made it
look like a bundle: the esbuild lane's real test app (semver+ms+mitt) had
32 bindings left after extraction and the split failed loud
(`no recognizable bundle wrapper`) although unpack had worked. Being a
bundled app is a property of the INPUT: the threshold now reads the
run's ORIGINAL bundle (`modules::wrapper::original_bundle_binding_count`),
while the tight wrapper GRAMMAR still reads the shipped text (a plain
script stays unsplittable however bundled its input was — pinned in
`emit/stable_split/stable_split_test.rs`).

This fixture pins both sides of that boundary:

- the ORIGINAL build clears the frozen threshold: **64 wrapper-scope
  bindings** (9 esbuild helpers, 24 `var require_modNN` CJS factories,
  24 `var import_modNN` interop bindings, and a small ESM app half incl.
  one lazy `__esm` module so the fossil split's grammar is exercised —
  an all-eager bundle fails its own `no __esm init definitions` gate);
- the POST-EXTRACTION runtime sits UNDER it: **40 bindings** (64 − 24
  vendored) — under the old gate this fixture's split failed loud, which
  is the red this fixture was born from;
- `modules_test::the_small_esbuild_fixture_pins_the_original_bundle_boundary`
  re-reads both counts off the committed build, so a regen that fattens
  the app half past 50 (or shrinks the vendor half under it) fails a
  unit test, not silently;
- a deterministic 26-line stdout report (`dep00: mod00:42` … `stamp:
stamp#007`): the e2e's bundle boot step proves the split tree behaves
  exactly like the input bundle (`node run.cjs` vs `node <input>` —
  same stdout, same exit).

The sibling `esbuild-bundle` fixture covers the OTHER side of the
boundary (a runtime that clears ≥50 on its own text). Run together they
pin that the gate read the input, and that the grammar never loosened.

## The build (PINNED: esbuild 0.27.2)

The committed `build/v<ver>/build/index.js` files are byte-reproducible.
Regenerate from the repo root (the source paths esbuild bakes into the
`__commonJS` object keys are relative to the build's working directory):

```bash
npx esbuild@0.27.2 test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/main.js \
  --bundle --format=iife \
  --outfile=test/e2e/fixtures/esbuild-bundle-small/build/v1.0.0/build/index.js
npx esbuild@0.27.2 test/e2e/fixtures/esbuild-bundle-small/source/v1.1.0/src/main.js \
  --bundle --format=iife \
  --outfile=test/e2e/fixtures/esbuild-bundle-small/build/v1.1.0/build/index.js
```

Verification history: 0.27.2 (2026-10-02 — the same version the
`esbuild-bundle` fixture and the TS-era exp075 reference verified
against).

## Why the SOURCE tree is committed here

Unlike `esbuild-bundle` (whose `source/` is local-only), this fixture's
`source/` IS committed: the regen commands above must actually run for
anyone cloning the repo, and this fixture exists precisely to keep a
boundary measurable — an unreproducible build cannot be re-counted when
the boundary pin fails. `.gitignore` carries the exception
(`!test/e2e/fixtures/esbuild-bundle-small/source/`).

## Why this fixture needs `"bundle": true` in its config

Same as `esbuild-bundle`: every leg runs WITH `--split`, the output is
the runnable CJS tree (`src/` + `vendor/` + `run.cjs`), the rename check
scans the whole tree, `--prior-version` points at the fresh run's
`.humanify/humanified.js`, and the boot step compares BEHAVIOR (stdout +
exit). The legacy-golden comparison is skipped (no pre-2026-09-28 binary
ever ran this pair), with a printed note.

The fixture's heads-up for future edits: keep the vendor half DOMINANT —
if the app half grows past ~9 more wrapper-scope bindings the runtime
crosses 50 and the fixture stops exercising the boundary (the unit pin
catches that). Grow the CJS deps (`source/v<ver>/src/vendor/modNN.cjs` —
one wrapper-scope `require_*` binding each), not the app half.
