# bun-lazy — a Bun app whose every module loads lazily

The `lazy` build of `../bun-apps` (recipe, source generator and sha256s
there): Bun 1.3.14, `bun build src/main.js --target=bun --format=cjs`,
unminified. All six ES modules are reached only through `require()` from a
CommonJS loader, and the entry is two statements — the shape the module
markers describe (Claude Code's: nearly every module lazy, a few-statement
entry tail). The six markers cover 99.6% of the app code.

## What it proves (finding #87)

The module-marker method still runs where it belongs: `--stats-json`
`splitMethod.method` = `module-markers`, each lazy module one file
(`expect.apart`), the tree boots with the input's behaviour — and the
fresh tree is byte-identical to main @ f616f33b's. Since the method is
chosen from the bundle's contents, the four older bundle fixtures (one
planted lazy module each, 17-45% coverage) take the fresh grouping; this is
the one real build that keeps the marker method under the e2e gate.
