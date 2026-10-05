# bun-apps — the source of the three plain-Bun-app fixtures

One small app, three ways of loading its modules, built by `build.sh` with
Bun 1.3.14 (`bun build src/main.js --target=bun --format=cjs`, unminified —
Bun's CommonJS output, the `// @bun @bun-cjs` wrapper):

| fixture     | how its six ES modules load                                        | lazy-init modules | split method (2026-10-05) |
| ----------- | ------------------------------------------------------------------ | ----------------- | ------------------------- |
| `bun-plain` | all imported statically — what a normal app does                   | 0                 | fresh grouping            |
| `bun-mixed` | 2 reached only through `require()` from a CommonJS loader, 4 eager | 2 (~45% of code)  | fresh grouping            |
| `bun-lazy`  | all 6 reached only through the loader; a two-statement entry       | 6 (>99% of code)  | module markers            |

Bun writes a lazy-init module (`var init_x = __esm(…)`) only for an ES
module that is `require()`d or loaded by `import()`. Claude Code loads
nearly everything that way, so its bundle is one long run of them, and the
split's module-marker method was chosen for every Bun bundle. A normal app
has none: on main @ f616f33b `bun-plain` exited 1 after naming ("the bundle
records no module fossils"), and `bun-mixed` piled its four eager modules
into one `src/index.js`. The split method is now chosen from what the
bundle contains (`crates/humanify-core/src/place/method.rs`, finding #87);
these three fixtures hold each case.

## The app

Six ES modules (`units`, `palette`, `archive`, `codec`, `ledger`, `router`):
each a chain of 40 small functions over its own table, filled by a
top-level loop (a side effect, so a required module gets a lazy init), and
a `<name>Report` that prints `<TAG>#<value>`. No module reads another, so
the reference graph has a seam at every module edge. Plus a local CommonJS
dependency (`src/vendor/clock.cjs`) and the real npm package **ms@2.1.3**
(vendor extraction), and a deterministic stdout report. The e2e's `apart`
expectations look the `<TAG>#` strings up in the split tree to prove which
module's code sits in which file.

v1.1.0 changes the palette and archive reports (zero-padded) and adds a
function to the CommonJS dependency.

## Regenerating

```bash
PATH="$HOME/.bun/bin:$PATH" bash test/e2e/fixtures/bun-apps/build.sh
```

`source/gen.mjs` writes each app version (deterministic: same arguments,
same bytes) into a scratch directory; `npm install` pins `ms`; each version
is built from inside its directory (Bun bakes `// src/…` comments relative
to the working directory). The script refuses any Bun but 1.3.14. Built
twice, identical sha256s (2026-10-05):

```
e271e9f4829739cb6013d4af4190caa999e780e7f9742c29c4a26fb130906555  bun-plain/build/v1.0.0/build/index.js
c3863f37ab10107d5358dd984b7352aa8981aaada520529cd15741f25df51961  bun-plain/build/v1.1.0/build/index.js
e23674d3e73ba7b3a31379f98e8bba67dda8f24c28e3337af174c09043c5dca5  bun-mixed/build/v1.0.0/build/index.js
58dda122ca713e5c8b5514bdfdfe8ae6a64b46906c949b30dafb42f9f98f6297  bun-mixed/build/v1.1.0/build/index.js
7901df80208a95bb71e96f7ec0186b06d07fac21e87891ea997b794e4d714ab8  bun-lazy/build/v1.0.0/build/index.js
419b260d20e7b205316fed35eeb125c92dedf7a8ed511f84c13dcf6536be41b9  bun-lazy/build/v1.1.0/build/index.js
```

This directory has no `fixture.config.json`, so the e2e stage does not run
it. `source/` and the fixtures' `build/` sit under `.gitignore`'s
`test/e2e/fixtures/*/source/` and `*/build/` patterns and are FORCE-ADDED,
like `node-app`'s: prettier, biome and knip skip ignored paths, so the
committed bytes are exactly what Bun read and wrote.
