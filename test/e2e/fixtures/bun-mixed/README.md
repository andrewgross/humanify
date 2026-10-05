# bun-mixed — a Bun app with some modules loaded lazily

The `mixed` build of `../bun-apps` (recipe, source generator and sha256s
there): Bun 1.3.14, `bun build src/main.js --target=bun --format=cjs`,
unminified. Of six ES modules, `archive` and `codec` are reached only
through `require()` from a CommonJS loader, so Bun wraps exactly those two
in lazy inits; `units`, `palette`, `ledger` and `router` stay eager. The two
markers cover ~45% of the app code (`humanify detect --split-method`).

## What it proves (finding #87, C2)

- **main @ f616f33b:** the module-marker method assumes every source file
  loads lazily. It gave each lazy module its file and put EVERYTHING after
  the last one — all four eager modules and the entry — into one
  538-line `src/index.js` (`UNITS#`, `PALETTE#`, `LEDGER#`, `ROUTER#` in one
  file).
- **now:** under the 99% coverage threshold the split takes the fresh
  grouping for the whole bundle, keeping each lazy module's end as a file
  boundary: six module files, one report each (`expect.apart`:
  `UNITS#`/`PALETTE#`, `PALETTE#`/`LEDGER#`, `LEDGER#`/`ROUTER#`,
  `ARCHIVE#`/`PALETTE#` in different files), and the tree boots with the
  input's behaviour.
