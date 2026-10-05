# bun-esm-minified — Bun's ES-module output, production shape (split skipped: spec I25)

A real Bun 1.3.14 build of `../node-app` with
`bun build --target=bun --format=esm --minify`.

**Why `--minify`:** a Bun app shipped to users is minified — Claude Code,
the only real Bun app this pipeline sees, is `bun build --minify` output —
and minifying costs this fixture nothing it needs: Bun is recognised by the
`// @bun` banner, which survives minification. The unminified Bun shape is
already covered (`bun-bundle`, CommonJS); the minified ESM build is the one
nobody had run.

## What it recorded before finding #87 (main @ 8883d0b5)

```
humanify detect --toolchain build/v1.0.0/build/index.js
bundler=bun (definitive, the banner), minifier=terser (tier unknown),
unpack adapter=bun, bundle layout=single-wrapper-function
```

Detection is right. The fresh `--split` run unpacks (26 vendor files
written) and names, then exits 1 with the same error as `esbuild-esm`:

```
Error: stable split failed before any tree was written: the run's input
bundle has no recognizable bundle wrapper (the frozen ≥50-binding gate is
measured on the input)
```

— Bun's `--format=esm` has top-level statements and a real
`import{basename}from"path"`, no wrapper function (spec I25, the P9
`BundleLayout` slot). No prior can be produced, so the v2 leg never runs.
A fix for ESM layouts has to read both bundlers' helpers: Bun's ESM
runtime differs from esbuild's (`__toESM` with WeakMap caches,
`var{getPrototypeOf:…}=Object` destructuring).

## Since finding #87 (2026-10-05): the split is skipped, not an error

A layout the split does not read no longer fails the run after naming. The
fresh run exits 0, prints

```
WARNING: --split skipped: the run's input bundle has no recognizable bundle
wrapper (…). The split reads one bundle layout, a single wrapper function
holding the program (docs/plugin-spec.md P9, I25); the named output is
written unsplit to <out>/runtime.js
```

and writes what a run without `--split` writes: the named `runtime.js` and
the extracted `vendor/`. `--stats-json` records
`splitMethod.method = "not-split"`; the v2 leg runs with that file as its
prior. The e2e holds the fixture to exactly that (`expect.splitMethod`):
fresh + prior + `--sequential` twice each, byte-deterministic, and no
runnable tree written. The layout itself is still open (spec I25): when a
second layout lands, this fixture's expectation becomes a split method and
the boot step runs its tree.

## The build (PINNED: Bun 1.3.14)

`bash test/e2e/fixtures/node-app/build.sh` (see its README).
`build/v<ver>/build/package.json` is `"type": "module"`. sha256 (2026-10-04):

- v1.0.0: `93e11ba722c1dfd99fbc97affead7c4d297fa7ee198fd6144705da4d3022a7ac`
- v1.1.0: `ccd6621427a443e44ec4988b2e117b5ba4ec82ea4beca925dd6b38a9d64ec7f1`
