# bun-plain — a Bun app with no lazily loaded module

The `plain` build of `../bun-apps` (recipe, source generator and sha256s
there): Bun 1.3.14, `bun build src/main.js --target=bun --format=cjs`,
unminified. Six ES modules imported statically, a local CommonJS
dependency and ms@2.1.3. Bun writes no lazy-init module, so the bundle
records no module markers.

## What it proves (finding #87, C1)

- **main @ f616f33b:** the split took the module-marker method because the
  BUNDLER is Bun, found no markers, and exited 1 after unpack and naming:
  `stable split failed before any tree was written: fossil split: the
bundle records no module fossils (no __esm init definitions)`.
- **now:** the split measures the markers (none), takes the fresh grouping
  (`--stats-json` `splitMethod.method` = `fresh-grouping`), and the tree
  boots with the input's behaviour. Each module's report lands in its own
  file (`expect.apart`).
