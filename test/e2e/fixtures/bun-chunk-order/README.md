# bun-chunk-order — a code-split Bun app whose runtime chunk loads lazily

The `order` app of `../bun-apps/source/gen.mjs`, built by `build.sh` with
**Bun 1.4.1** (`bun build src/main.js --compile --splitting --format esm
--minify`) and unpacked with bun-decompile. The entry loads both of its
modules through `import()` and requires nothing itself, so Bun puts its
runtime helpers (`__toESM`, `__commonJS`) in a chunk that is ALSO loaded
lazily:

- `alpha` default-imports a Node built-in (`import os from "node:os"`) and
  a CommonJS module. The linker writes the built-in's import in Bun's
  CommonJS shape, `__toESM(require("os"))`, and `__toESM` reads values
  (`Object.create`, …) that the runtime chunk's init sets.
- `beta` imports a dependency that ships pre-bundled: its own CommonJS
  helper is declared in ONE `var` together with values that read
  `Object`, and its wrapped modules (`var x = helper(fn)`) sit in a later
  statement.

These are the two shapes that broke Claude Code's linked program from
2.1.248 and from 2.1.269 (exp218).

## What it proves (exp218)

On main `6477bf247` the split tree fails at load (`<helper> is not a
function`: the wrapped module's `var` stayed at the top level, its helper
moved into the chunk's init). With exp218's linker every statement of a
lazily loaded chunk runs in its init, after the inits of the chunks it
imports and before nothing that uses it; the fresh, prior and sequential
split trees print exactly what the input prints. The input itself is run
by Node with the bunfs resolve hook (as `bun-chunks`). v1.1.0 changes the
alpha report.

## Regenerating

```bash
BUN=/path/to/bun-1.4.1 BUN_DECOMPILE=/path/to/bun-decompile \
  bash test/e2e/fixtures/bun-chunk-order/build.sh
```

Built twice, identical sha256s (2026-11-12):

```
011f0cffbcdc0ea9cc89c00ec4a802680d34829651970640f2eef3dd3a299f70  build/v1.0.0/build/alpha-a2bmdhd8.js
e10e0a257874d4ff8b15c9e71f6d5202b7ead15c8aa878ff5849410708eeda61  build/v1.0.0/build/beta-dzz19h3j.js
54ea2d7c42a78e6122a28fd3365dd333b9a6cf643e2f2147eb3f612cc0d50d13  build/v1.0.0/build/index.js
1363f89452e4838aeee49cce76612ffbebf8ee174272a4e3b39b2a2c80164ff3  build/v1.0.0/build/main-p04hftp1.js
7deb71a2b1c0888528ae86b562c33c88c1c58da6a037c7093f94bdd299dfbc6f  build/v1.1.0/build/alpha-03cds5kf.js
e10e0a257874d4ff8b15c9e71f6d5202b7ead15c8aa878ff5849410708eeda61  build/v1.1.0/build/beta-dzz19h3j.js
07eec719a0d4034111d79d1d9c9fa1dce5d95265cc058987be0087be920b023e  build/v1.1.0/build/index.js
1363f89452e4838aeee49cce76612ffbebf8ee174272a4e3b39b2a2c80164ff3  build/v1.1.0/build/main-p04hftp1.js
```
