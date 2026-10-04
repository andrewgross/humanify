# node-app — the shared source of the non-Bun-CJS fixtures

One small Node app, two versions, built four ways by `build.sh`:

| fixture            | bundler        | command                                           | what it records today                    |
| ------------------ | -------------- | ------------------------------------------------- | ---------------------------------------- |
| `esbuild-minified` | esbuild 0.27.2 | `--bundle --minify --format=iife --platform=node` | KNOWN GAP: detected as unknown (spec I2) |
| `esbuild-cjs`      | esbuild 0.27.2 | `--bundle --format=cjs --platform=node`           | KNOWN GAP: `--split` fails (spec I25)    |
| `esbuild-esm`      | esbuild 0.27.2 | `--bundle --format=esm --platform=node`           | KNOWN GAP: `--split` fails (spec I25)    |
| `bun-esm-minified` | Bun 1.3.14     | `build --target=bun --format=esm --minify`        | KNOWN GAP: `--split` fails (spec I25)    |

Every eval pair is Claude Code, which is one shape: Bun, CommonJS wrapper,
minified. These four are the shapes the pipeline had never been run on.

## The app

`bun-bundle`'s app (same 24 local CommonJS deps, the real npm package
**ms@2.1.3**, a CommonJS module that `require()`s an ES module with a
top-level side effect so both bundlers write a lazy `__esm` init, a dynamic
`import()`, a deterministic stdout report) plus ONE line: an import of the
Node builtin `node:path`. Both bundlers keep a builtin EXTERNAL, so the ESM
builds carry a real top-level `import` statement — what an ES-module app
bundle actually looks like (without it, an ESM build of this app would hold
no `import`/`export` at all and parse as a plain script).

v1.1.0 is the same real source change as `bun-bundle`'s: the last dep gains
`tripled`, the late module zero-pads its output, the lazy stamp moves.

## Regenerating

```bash
bash test/e2e/fixtures/node-app/build.sh
```

Builds each version from a scratch copy (`npm install` pins `ms` exactly),
from inside the version's directory — both bundlers bake source paths
relative to the working directory into the output. esbuild comes from
`npx esbuild@0.27.2`; the script refuses any Bun but 1.3.14. It also writes
each build's `package.json` (`"type": "commonjs"` for the iife and cjs builds,
`"module"` for the ESM ones): the repo root is `"type": "module"`, so
without it Node would load those builds as ESM, where esbuild's
`require()` shim throws. Byte-reproducible: three consecutive runs printed
identical sha256s (2026-10-04); the fixtures' READMEs record them.

This directory has no `fixture.config.json`, so the e2e stage does not run
it; it is the four fixtures' source. `source/` and every fixture's
`build/` sit under `.gitignore`'s `test/e2e/fixtures/*/source/` and
`*/build/` patterns and are FORCE-ADDED, like `bun-bundle`'s: prettier,
biome and knip (which would otherwise report the app's `ms` import as an
unlisted dependency) all skip ignored paths, so the committed bytes are
exactly what the bundlers read and wrote.
