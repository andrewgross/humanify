# exp129: a copies layer built from the installed packages, and transpiler helpers that seed no package

## STATUS (2026-10-18)

DONE on branch `exp129-helpers-copies` (origin/main `bd24cb3c` merged in: exp125 scorer 10, exp128 scorer 11, exp130).
Final scoring uses the recommended index k2-all-r5-origin plus its helper table (`k2-all-r5-origin-h`).

- **A. Scorer 12 = copies layer v2 + rule `patched`.** Every committed label (415) was re-scored once.
  - **The copies layer v2.** exp104's rule (code + declared dependency + the shipped file naming the origin + one
    passing origin) is unchanged. The evidence now comes from:
    - the INSTALLED packages (the shipper's own `package.json`, the shipped file);
    - every row table of the evidence indexes, with the commonness bound applied per table;
    - every judged unit, factory body statements included.

    It no longer comes from one reference label's run on one slice. A module specifier (`from "O"`, `require("O")`)
    no longer counts as naming O. The layer's copy bytes grow on every corpus (dev 31 KB → 815 KB, remin 15 KB →
    1.43 MB, test 789 KB → 3.7 MB).
  - **`patched`** (exp130's declared builder/truth difference): a module-marker path may hold `=`, which is how pnpm
    spells a patched dependency. The new layer `origins12` changes 15 entries' origin layers. With the rule off, the
    layer is the scorer 10 layer byte for byte.
  - **Ledger (both directions, every label)**: see §1.3.
    - WRONG→GOOD dominates.
    - Dev right→wrong is 1,330 B (apps). Both cases are files that `require`/`import` the old origin, so the old
      credit was false.
    - test2 right→wrong is about 128 KB (test2-bun 102 KB, test2-rollup 26 KB). In aggregate (no names; test2 is
      never diagnosed) the lost v1 ranges are "named only by a module specifier", the same rule. On dev, all 13
      such cases checked are files that import the origin at runtime.
    - The headline cost: **test2-bun P 98.85 → 98.54 (-0.30)**. test2-esbuild P is +0.89.
  - **Hand-check:** 20/20 relabelled dev units are right (seed 129, `handcheck.json`). The check covers new origins
    (vitepress/markdown-it, @hot-loader/react-dom/react-dom, jsonpath-plus/jsep, @polka/compression), `patched`
    regions (sirv in vite, @eslint/config-array) and one lost credit (d/es5-ext, which d `require`s).
  - With both changes off, scorer 12 reproduces scorer 11's cards exactly (`verify_s11.py`, 3 labels).
- **B. Index helper table + `[library.discovery] helper_prints = off | seed | credit` (default off; commit
  `096d89e08`).**
  - The merge keeps the prints of the helper SOURCES' own builds (tslib, @babel/runtime, @swc/helpers,
    regenerator-runtime, @oxc-project/runtime) as `helpers.jsonl`.
  - `seed`: a unit whose WHOLE print that table holds seeds no other package. `credit`: the seed becomes the helper
    package's.
  - Gate 13/13 bare. Warm neutrality of `096d89e08` vs main `1c3f25574`: NEUTRAL 0/0 +0/+0, in default mode and in
    report mode. `library-report.json` is byte-identical.
  - **Under scorer 12, on both scopes, `seed` vs off (k2-all-r5-origin-h):**
    - P is never lower, on 21 sets.
    - F0.5 rises on 10 sets: remin-esbuild +0.22 (statement scope), test2-esbuild +0.10, apps-esbuild +0.07, …
    - F0.5 falls only on test-webpack (-0.001) and test2-webpack (-0.000), where about 25 KB of right webpack seeds
      go.
  - **Recommendation: `seed`, not flipped.** That is Andrew's call: there is a sub-0.001 F0.5 loss on two webpack
    test sets. `credit` loses recall on test2-esbuild, pop-esbuild and test2-webpack (scorer 10, §2.3).
- **C. `terser_slices = auto`, re-judged under scorer 12.** auto+seed against seed alone is clean on every set:
  P and F0.5 are never lower, on both scopes.
  - wrangler: F0.5 +10.7 (totals) / +13.3 (statement scope), P +1.78.
  - remin-terser: P 99.82 → **99.90**, F0.5 +6.0 (totals) / +7.9 (statement scope).
  - Against off, auto+seed lowers no P, and F0.5 only by the -0.001 above. auto ALONE still lowers remin-terser P
    (-0.08: the tslib helpers), so **auto is clean only together with `helper_prints = seed`.**
  - Recommendation: flip both together (`helper_prints = "seed"` + `terser_slices = "auto"`). This needs a warm
    neutrality run on the flip and Andrew's call on the two webpack test sets.

## 1. The copies layer v2 (scorer 12)

### 1.1 What exp104's layer did, and where it fell short

exp104's copies layer (`experiments/104-truth-audit/copies.py`) gives a unit an ORIGIN when the unit is a copy of
another package O, inlined by the shipper S. The rule has four tests: code, dependency, named and unique.

The layer was built from one REFERENCE LABEL per corpus. That label's run fixed which units were examined, and its
index supplied the code evidence, read at the bundle's slice only. That caused three gaps:

- **One slice.** A terser-minified bundle's inlined copy matches only the terser-built slices (exp127). So
  `onnxruntime-web` inlined in `transformers.min.js` was never credited. That is exp127's 14.3 KB "truth gap" on
  remin-terser.
- **One index.** A copy needs O's own rows. A newer index holds packages the reference never built. It also holds
  an origin's code in more sponges, and the commonness bound ("a print more than 3 families hold is helper code")
  depends on one table's population.
- **A registry cache.** The dependency test read a registry cache that lacked many versions: `noManifest` was
  870 KB on apps.

### 1.2 What v2 does (`copies2.py`)

The rule is exp104's, and it asks the same questions. What changed is where each test reads its evidence:

- **units:** every judged unit with prints, meaning statements, lazy bodies and FACTORY BODY statements. They are
  read from the unit model of some runs (`unitRows`; no claim, veto or setting is read). Per corpus, a default run
  and a `factory_bodies = true` run are joined by span (§4), because exp128's factory scope is most of an esbuild
  bundle.
- **code:** judged in each ROW TABLE of the evidence indexes, with the commonness bound applied per table. The best
  cover wins. The tables are:
  - every slice of k2-all-r5 and of the recommended k2-all-r5-origin, each with its helper table (§2: a bundled tslib is a copy like any other);
  - every slice of `step4-wr`. It holds the step-4 slices that each frozen exp104 layer was built on, so v2 keeps
    what those layers saw.
- **dependency:** the INSTALLED `node_modules/<S>/package.json` at the path the source map names, used when its
  version is the truth's. The registry caches of the earlier lanes are only the fallback.
- **named:** the shipped file (sourcesContent, else the installed file), with one change. **A module specifier
  does not count** (`from "O"`, `import "O"`, `require("O")`, `import("O")`, subpaths included). A file that imports
  a package is its consumer, so the package's code is not in that file.
  - Without this, a first v2 pass credited Angular's `ɵfac` boilerplate in `@ng-bootstrap/ng-bootstrap`'s fesm to
    `@angular/common`, which that file imports.
  - `from` and `import` take quotes only, so a JSDoc "from \`path-to-regexp\`" is still a name.
- **unique, mapped:** unchanged.

The layer is frozen as byte ranges: `<DATA>/truth/<id>.copies2.json.gz`, with the shas in `copies2*.json`.
`truth_audit.COPIES2["on"]` selects the layer. With it off, scorer 12 reproduces scorer 11's cards
(`verify_s11.py`).

### 1.3 What scorer 12 changes

**The layer.** Copy bytes per corpus, v1 → v2:

| corpus | v1 | v2 |
| --- | --- | --- |
| dev | 30,678 | 815,302 |
| apps | 347,449 | 1,026,268 |
| bundlers | 521 | 6,456 |
| pop | 1,410,440 | 2,080,742 |
| remin | 14,548 | 1,431,123 |
| test | 788,724 | 3,735,770 |
| test2 | 669,341 | 1,712,921 |

Most of the growth is factory body statements, which v1 never examined, and the terser-built and k2 slices. The
derivation's per-test byte counts are in each `copies2*.json` (`bytes`).

**`patched`.** origins12 differs from origins10 on 15 entries: dev 2, apps 2, pop 8, test 3, test2 0.

**Scorer 11 → 12 on main's defaults (k2-all-r5-origin, labels `*exp130-origin*`).** P / F0.5 on the headline
(both scopes):

| cc | 100.00 / 95.77 | 100.00 / 95.77 | +0.000 / +0.000 | 0 -> 0 |
| esbuild-app | 96.04 / 51.67 | 96.33 / 51.83 | +0.284 / +0.152 | 306,091 -> 317,637 |
| heldout | 99.31 / 32.06 | 99.31 / 32.06 | +0.000 / +0.000 | 3,847,063 -> 4,143,789 |
| wrangler | 96.45 / 30.31 | 96.45 / 30.31 | +0.000 / +0.000 | 35,441 -> 40,388 |
| apps-esbuild | 99.77 / 47.34 | 99.79 / 47.35 | +0.013 / +0.006 | 5,698,998 -> 5,837,600 |
| apps-rollup | 99.06 / 94.35 | 99.09 / 94.38 | +0.031 / +0.029 | 1,906,094 -> 2,120,700 |
| apps-webpack | 99.39 / 92.80 | 99.58 / 92.98 | +0.194 / +0.181 | 681,640 -> 800,417 |
| dev-rollup | 99.87 / 94.43 | 99.87 / 94.43 | +0.000 / +0.000 | 1,693 -> 7,628 |
| dev-webpack | 99.64 / 89.22 | 99.64 / 89.22 | +0.000 / +0.000 | 1,077 -> 1,077 |
| pop-bun | 98.79 / 63.85 | 98.91 / 63.93 | +0.128 / +0.082 | 35,686,180 -> 36,153,242 |
| pop-esbuild | 98.59 / 64.79 | 99.02 / 65.07 | +0.431 / +0.283 | 33,950,332 -> 34,541,212 |
| remin-esbuild | 99.04 / 22.53 | 99.04 / 22.53 | +0.000 / +0.000 | 153,753 -> 326,582 |
| remin-terser | 99.82 / 19.59 | 99.82 / 19.59 | +0.000 / +0.000 | 140,916 -> 421,603 |
| test-bun | 99.31 / 48.29 | 99.33 / 48.29 | +0.018 / +0.009 | 32,243,277 -> 32,847,702 |
| test-esbuild | 99.10 / 51.33 | 99.12 / 51.33 | +0.014 / +0.007 | 31,950,893 -> 32,193,355 |
| test-rollup | 98.68 / 91.94 | 98.96 / 92.21 | +0.281 / +0.262 | 23,606,958 -> 24,055,731 |
| test-webpack | 99.48 / 92.49 | 99.48 / 92.49 | +0.000 / +0.000 | 14,123,935 -> 15,705,157 |
| test2-bun | 98.85 / 43.15 | 98.54 / 43.02 | -0.304 / -0.133 | 7,724,285 -> 7,922,736 |
| test2-esbuild | 98.56 / 39.77 | 99.44 / 40.13 | +0.887 / +0.358 | 5,889,671 -> 6,313,127 |
| test2-rollup | 96.34 / 91.35 | 96.35 / 91.35 | +0.007 / +0.007 | 4,651,432 -> 4,750,271 |
| test2-webpack | 99.27 / 91.64 | 99.27 / 91.64 | +0.001 / +0.001 | 6,109,459 -> 6,134,913 |

**Ledger** (`ledger12.py`, both directions, every committed label; `ledger-<corpus>.json`, aggregates only on test
and test2): LEDGER

The dev right→wrong cases are `history`→`resolve-pathname` (1,119 B) and `d`→`es5-ext` (211 B). Both shipped files
import the old origin (`import resolvePathname from 'resolve-pathname'`, `require("es5-ext/…")`), so exp104's credit
came through the module specifier.

The lost v1 ranges, by the first v2 test that fails (`lost_reasons`, aggregate bytes):

- test2: named only by a module specifier 144,843; label changed (scorer 11's `notinstalled`) 21,829; the origin
  layer gives one 6,708; not named 4,200; other 3,719.
- test: the origin layer gives one 128,496.
- pop: the origin layer gives one 73,004; other 13,364; specifier 300.
- apps: the origin layer gives one 48,081; label changed 17,430; specifier 7,956; other 3,589.

On apps, every one of the 13 specifier-only cases was read: magic-string importing @jridgewell/sourcemap-codec,
autoprefixer requiring postcss, cheerio-select requiring css-select, @jimp/plugin-resize requiring zod, recast
requiring ast-types, and so on. Each file is the origin's consumer, so its code is not inlined there.

## 2. Transpiler helpers: the helper table and `helper_prints`

### 2.1 The class

tsc and Babel emit their helpers (`__awaiter`, `__generator`, `__extends`, `_typeof` …) inline. A bundler inlines
tslib's `tslib.es6.mjs` into a package's dist. Either way, the same statements sit in many packages' index rows.

The helper packages themselves are stop-listed, so the index dropped their rows at merge. That meant the index could
not tell canonical helper code from a package's own code. exp106's `helper_families` infers commonness from the
index, and that cannot see these helpers (exp127: 8 or 12 still loses to module propagation).

Probe, before any code (main labels, scorer 9). The probe split the claims on units that a helper source's own
build holds:

- **Units whose WHOLE statement a helper source's build holds.** As seeds they are almost all WRONG:
  - wrong: 1.8 KB on dev, 34.7 KB on apps, 23.1 KB on pop, 22.5 KB on remin-auto. The truth there is the real
    tslib / @babel/runtime module the app imports; the claim was `@exasol/exasol-driver-ts`, rollup,
    `@napi-rs/wasm-runtime`, `react-to-print` …;
  - right: 0.7 KB.
  - Claims that reach the same units by propagation (DOWN / MODULE) are mostly right (primevue's Babel helpers
    inside its own modules).
- **Units that a helper print covers only as a FUNCTION** (tsc's `var __x = (this && this.__x) || function`). These
  are mostly the inliner's own right claims: puppeteer-core 39–48 KB, rxjs, …. They are left alone.

@babel/helpers is no helper source: its dist carries @babel/types and @babel/template code. Nor are the core-js
runtimes: @babel/runtime-corejs* re-export core-js-pure, and as sources they took 211 KB of right core-js claims in
the probe.

### 2.2 Code (test-first; commit `096d89e08`)

- **Builder:** `stoplists.json` gets a new `helperSources` list. The merge (`humanify_index::merge`) still drops
  those packages, but it writes their unit hashes, over every slice, to `helpers.jsonl` (`HelperRow {hash,
  owners}`), and the manifest lists the table. An index with no helper-source rows writes no table.
- **Lookup:** `LibraryIndex::helper_owners(hash)` reads the table when the manifest lists it.
  `LibraryIndex::with_helpers` is the tests' constructor.
- **Discovery:** `HelperPrints::{Off, Seed, Credit}` and `Containment::helper_seed`. It is applied to index seeds
  and to queue-index seeds:
  - a seed on a unit whose whole print (at or over the row floor) the helper table holds is kept when the seed's
    family is a holder;
  - otherwise it is dropped (`seed`), or renamed to the ONE helper family holding it (`credit`; two families hold
    it: dropped).
  - Propagation is untouched.
- **Config:** `[library.discovery] helper_prints`, default `"off"`. Documented in the example toml,
  docs/library-matching-config.md, docs/design/library-index.md §7, and docs/responsibility.md.
- **Index:** `/work/exp129-data/ix/k2-all-r5h`, index id `e7e2dc34…`. It is a merge-only rebuild of exp124's
  k2-all-r5 store with this branch's builder. The manifest differs from k2-all-r5 ONLY in
  `tables["helpers.jsonl"]` (5,672 hashes) and in `data["stoplists.json"]`. Every lookup table is byte-identical,
  so `off` on k2-all-r5h claims exactly what main claims on k2-all-r5.

### 2.3 Results (scorer 10, k2-all-r5h, main defaults; off = exp127-off, test2 off = test2-exp124-r5)

P / F0.5, with the delta against off in brackets. App claimed is 0 everywhere. `auto` means
`terser_slices = auto, terser_share = 0.2` (exp127). auto alone equals off on test2 (exp127).

| set | schema | off P / F0.5 | seed P / F0.5 | credit P / F0.5 | auto P / F0.5 | auto-seed P / F0.5 | auto-credit P / F0.5 |
|---|---|---|---|---|---|---|---|
| cc | 10 | 100.00 / 95.77 | 100.00 / 95.77 (+0.000 / +0.000) | 100.00 / 95.77 (+0.000 / +0.000) | 100.00 / 95.77 (+0.000 / +0.000) | 100.00 / 95.77 (+0.000 / +0.000) | 100.00 / 95.77 (+0.000 / +0.000) |
| esbuild-app | 10 | 96.60 / 89.59 | 96.65 / 89.62 (+0.047 / +0.032) | 96.65 / 89.63 (+0.047 / +0.037) | 96.60 / 89.59 (+0.000 / +0.000) | 96.65 / 89.62 (+0.047 / +0.032) | 96.65 / 89.63 (+0.047 / +0.037) |
| heldout | 10 | 99.31 / 90.79 | 99.31 / 90.79 (+0.000 / +0.000) | 99.31 / 90.79 (+0.000 / +0.000) | 99.31 / 90.79 (+0.000 / +0.000) | 99.31 / 90.79 (+0.000 / +0.000) | 99.31 / 90.79 (+0.000 / +0.000) |
| wrangler | 10 | 95.99 / 52.28 | 95.99 / 52.28 (+0.000 / +0.000) | 95.99 / 52.28 (+0.000 / +0.000) | 98.01 / 65.85 (+2.020 / +13.573) | 98.01 / 65.85 (+2.020 / +13.573) | 98.01 / 65.85 (+2.020 / +13.573) |
| apps-esbuild | 10 | 99.29 / 97.78 | 99.39 / 97.85 (+0.095 / +0.074) | 99.39 / 97.86 (+0.095 / +0.079) | 99.29 / 97.78 (+0.000 / +0.000) | 99.39 / 97.85 (+0.095 / +0.074) | 99.39 / 97.86 (+0.095 / +0.079) |
| apps-rollup | 10 | 98.98 / 94.12 | 99.00 / 94.13 (+0.020 / +0.014) | 99.00 / 94.14 (+0.020 / +0.017) | 98.98 / 94.12 (+0.000 / +0.000) | 99.00 / 94.13 (+0.020 / +0.014) | 99.00 / 94.14 (+0.020 / +0.017) |
| apps-webpack | 10 | 99.39 / 92.79 | 99.41 / 92.80 (+0.018 / +0.013) | 99.41 / 92.80 (+0.018 / +0.014) | 99.39 / 92.79 (+0.000 / +0.000) | 99.41 / 92.80 (+0.018 / +0.013) | 99.41 / 92.80 (+0.018 / +0.014) |
| dev-rollup | 10 | 99.87 / 94.43 | 99.87 / 94.43 (+0.000 / +0.000) | 99.87 / 94.43 (+0.000 / +0.000) | 99.87 / 94.43 (+0.000 / +0.000) | 99.87 / 94.43 (+0.000 / +0.000) | 99.87 / 94.43 (+0.000 / +0.000) |
| dev-webpack | 10 | 99.64 / 89.22 | 99.64 / 89.22 (+0.000 / +0.000) | 99.64 / 89.22 (+0.000 / +0.000) | 99.64 / 89.22 (+0.000 / +0.000) | 99.64 / 89.22 (+0.000 / +0.000) | 99.64 / 89.22 (+0.000 / +0.000) |
| pop-bun | 10 | 95.51 / 91.77 | 95.51 / 91.77 (+0.000 / +0.000) | 95.51 / 91.77 (+0.000 / +0.000) | 95.51 / 91.77 (+0.000 / +0.000) | 95.51 / 91.77 (+0.000 / +0.000) | 95.51 / 91.77 (+0.000 / +0.000) |
| pop-esbuild | 10 | 94.08 / 91.24 | 94.10 / 91.26 (+0.026 / +0.019) | 94.10 / 91.26 (+0.024 / +0.014) | 94.08 / 91.24 (+0.000 / +0.000) | 94.10 / 91.26 (+0.026 / +0.019) | 94.10 / 91.26 (+0.024 / +0.014) |
| remin-esbuild | 10 | 99.04 / 94.02 | 99.35 / 94.24 (+0.301 / +0.217) | 99.35 / 94.27 (+0.302 / +0.243) | 99.05 / 94.11 (+0.003 / +0.083) | 99.35 / 94.32 (+0.304 / +0.300) | 99.35 / 94.35 (+0.305 / +0.326) |
| remin-terser | 10 | 99.82 / 88.32 | 99.82 / 88.32 (+0.000 / +0.000) | 99.82 / 88.32 (+0.000 / +0.000) | 99.50 / 95.68 (-0.325 / +7.364) | 99.66 / 95.80 (-0.161 / +7.485) | 99.67 / 95.81 (-0.155 / +7.500) |
| test-bun | 10 | 97.85 / 89.80 | 97.85 / 89.80 (+0.000 / +0.000) | 97.85 / 89.80 (+0.000 / +0.000) | 97.85 / 89.80 (+0.000 / +0.000) | 97.85 / 89.80 (+0.000 / +0.000) | 97.85 / 89.80 (+0.000 / +0.000) |
| test-esbuild | 10 | 97.00 / 92.16 | 97.05 / 92.19 (+0.044 / +0.032) | 97.05 / 92.19 (+0.044 / +0.036) | 97.00 / 92.16 (+0.000 / +0.000) | 97.05 / 92.19 (+0.044 / +0.032) | 97.05 / 92.19 (+0.044 / +0.036) |
| test-rollup | 10 | 97.94 / 90.29 | 97.94 / 90.29 (+0.005 / +0.003) | 97.94 / 90.30 (+0.005 / +0.006) | 97.94 / 90.29 (+0.000 / +0.000) | 97.94 / 90.29 (+0.005 / +0.003) | 97.94 / 90.30 (+0.005 / +0.006) |
| test-webpack | 10 | 99.24 / 91.80 | 99.25 / 91.80 (+0.007 / -0.001) | 99.25 / 91.80 (+0.007 / -0.000) | 99.24 / 91.80 (+0.000 / +0.000) | 99.25 / 91.80 (+0.007 / -0.001) | 99.25 / 91.80 (+0.007 / -0.000) |
| test2-bun | 10 | 98.85 / 94.21 | 98.85 / 94.21 (+0.000 / +0.000) | 98.85 / 94.21 (+0.000 / +0.000) | =off (exp127) | 98.85 / 94.21 (+0.000 / +0.000) | 98.85 / 94.21 (+0.000 / +0.000) |
| test2-esbuild | 10 | 99.49 / 93.64 | 99.63 / 93.74 (+0.140 / +0.099) | 99.63 / 93.68 (+0.139 / +0.047) | =off (exp127) | 99.63 / 93.74 (+0.140 / +0.099) | 99.63 / 93.68 (+0.139 / +0.047) |
| test2-rollup | 10 | 96.33 / 91.25 | 96.33 / 91.25 (+0.004 / +0.003) | 96.34 / 91.27 (+0.018 / +0.020) | =off (exp127) | 96.33 / 91.25 (+0.004 / +0.003) | 96.34 / 91.27 (+0.018 / +0.020) |
| test2-webpack | 10 | 99.27 / 91.64 | 99.28 / 91.64 (+0.007 / -0.000) | 99.28 / 91.63 (+0.007 / -0.006) | =off (exp127) | 99.28 / 91.64 (+0.007 / -0.000) | 99.28 / 91.63 (+0.007 / -0.006) |

- **`seed`:**
  - P is never lower, on 21 sets.
  - F0.5 rises on esbuild-app, all three apps sets, pop-esbuild, remin-esbuild (+0.22), test-esbuild,
    test-rollup, test2-esbuild (+0.10) and test2-rollup.
  - F0.5 falls only on test-webpack (-0.0008) and test2-webpack (-0.0004): about 25 KB of right claims in webpack
    bundles were seeds that no propagation recovers. Not diagnosed: test2 is never diagnosed.
  - Recall is otherwise unchanged. The rule removes wrong seeds.
- **`credit`:** it buys a little recall on the dev sets (remin-esbuild +0.09 R). But it loses recall on
  pop-esbuild, test2-esbuild (-0.17 R, F0.5 +0.05 against seed's +0.10) and test2-webpack. A tslib claim is right
  only where the truth is the real tslib module.
- **Recommendation: `seed`.** It is not flipped, because test-webpack and test2-webpack F0.5 fall by under 0.001.
  That is Andrew's call.

## 3. `terser_slices = auto`, re-judged (part C)

exp127 left `auto` off because remin-terser P fell from 99.82 to 99.50. The added wrong bytes were 14.3 KB of
onnxruntime-web (a truth gap) and 10.5 KB of tslib helpers. Both are fixed:

- **Under scorer 10 (old copies layer):** auto+seed on remin-terser is 99.66 / 95.80, still below off's P of
  99.82, because onnxruntime-web's copy is not in the truth.
- **Under scorer 12 (copies v2):** the onnxruntime-web units carry the origin onnxruntime-common, which is in
  onnxruntime-web's repository family. auto+seed then reads **99.89 / 96.02** against off's 99.82 / 88.32.

### 3.1 Parts B and C under scorer 12 (k2-all-r5-origin-h, main defaults; off = `*exp130-origin*`)

`auto` = `terser_slices = auto, terser_share = 0.2`. Deltas are against off. App claimed is 0 everywhere.

**Totals (both scopes, the headline from scorer 11):**

| set | schema | off P / F0.5 | seed P / F0.5 | auto P / F0.5 | auto-seed P / F0.5 |
|---|---|---|---|---|---|
| cc | 12 | 100.00 / 95.77 | 100.00 / 95.77 (+0.000 / +0.000) | 100.00 / 95.77 (+0.000 / +0.000) | 100.00 / 95.77 (+0.000 / +0.000) |
| esbuild-app | 12 | 96.33 / 51.83 | 96.37 / 51.84 (+0.047 / +0.011) | 96.33 / 51.83 (+0.000 / +0.000) | 96.37 / 51.84 (+0.047 / +0.011) |
| heldout | 12 | 99.31 / 32.06 | 99.31 / 32.06 (+0.000 / +0.000) | 99.31 / 32.06 (+0.000 / +0.000) | 99.31 / 32.06 (+0.000 / +0.000) |
| wrangler | 12 | 96.45 / 30.31 | 96.45 / 30.31 (+0.000 / +0.000) | 98.23 / 41.04 (+1.779 / +10.732) | 98.23 / 41.04 (+1.779 / +10.732) |
| apps-esbuild | 12 | 99.79 / 47.35 | 99.88 / 47.37 (+0.096 / +0.017) | 99.79 / 47.35 (+0.000 / +0.000) | 99.88 / 47.37 (+0.096 / +0.017) |
| apps-rollup | 12 | 99.09 / 94.38 | 99.11 / 94.40 (+0.020 / +0.014) | 99.09 / 94.38 (+0.000 / +0.000) | 99.11 / 94.40 (+0.020 / +0.014) |
| apps-webpack | 12 | 99.58 / 92.98 | 99.60 / 92.99 (+0.018 / +0.013) | 99.58 / 92.98 (+0.000 / +0.000) | 99.60 / 92.99 (+0.018 / +0.013) |
| dev-rollup | 12 | 99.87 / 94.43 | 99.87 / 94.43 (+0.000 / +0.000) | 99.87 / 94.43 (+0.000 / +0.000) | 99.87 / 94.43 (+0.000 / +0.000) |
| dev-webpack | 12 | 99.64 / 89.22 | 99.64 / 89.22 (+0.000 / +0.000) | 99.64 / 89.22 (+0.000 / +0.000) | 99.64 / 89.22 (+0.000 / +0.000) |
| pop-bun | 12 | 98.91 / 63.93 | 98.91 / 63.93 (+0.000 / +0.000) | 98.91 / 63.93 (+0.000 / +0.000) | 98.91 / 63.93 (+0.000 / +0.000) |
| pop-esbuild | 12 | 99.02 / 65.07 | 99.05 / 65.08 (+0.026 / +0.009) | 99.02 / 65.07 (+0.000 / +0.000) | 99.05 / 65.08 (+0.026 / +0.009) |
| remin-esbuild | 12 | 99.04 / 22.53 | 99.35 / 22.54 (+0.301 / +0.012) | 99.05 / 22.61 (+0.004 / +0.084) | 99.35 / 22.62 (+0.304 / +0.097) |
| remin-terser | 12 | 99.82 / 19.59 | 99.82 / 19.59 (+0.000 / +0.000) | 99.74 / 25.58 (-0.083 / +5.997) | 99.90 / 25.59 (+0.079 / +6.005) |
| test-bun | 12 | 99.33 / 48.29 | 99.33 / 48.29 (+0.000 / +0.000) | 99.33 / 48.29 (+0.000 / +0.000) | 99.33 / 48.29 (+0.000 / +0.000) |
| test-esbuild | 12 | 99.12 / 51.33 | 99.16 / 51.34 (+0.042 / +0.009) | 99.12 / 51.33 (+0.000 / +0.000) | 99.16 / 51.34 (+0.042 / +0.009) |
| test-rollup | 12 | 98.96 / 92.21 | 98.97 / 92.21 (+0.005 / +0.003) | 98.96 / 92.21 (+0.000 / +0.000) | 98.97 / 92.21 (+0.005 / +0.003) |
| test-webpack | 12 | 99.48 / 92.49 | 99.48 / 92.48 (+0.007 / -0.001) | 99.48 / 92.49 (+0.000 / +0.000) | 99.48 / 92.48 (+0.007 / -0.001) |
| test2-bun | 12 | 98.54 / 43.02 | 98.54 / 43.02 (+0.000 / +0.000) | 98.54 / 43.02 (+0.000 / +0.000) | 98.54 / 43.02 (+0.000 / +0.000) |
| test2-esbuild | 12 | 99.44 / 40.13 | 99.58 / 40.15 (+0.140 / +0.018) | 99.44 / 40.13 (+0.000 / +0.000) | 99.58 / 40.15 (+0.140 / +0.018) |
| test2-rollup | 12 | 96.35 / 91.35 | 96.36 / 91.36 (+0.004 / +0.003) | 96.35 / 91.35 (+0.000 / +0.000) | 96.36 / 91.36 (+0.004 / +0.003) |
| test2-webpack | 12 | 99.27 / 91.64 | 99.28 / 91.64 (+0.007 / -0.000) | 99.27 / 91.64 (+0.000 / +0.000) | 99.28 / 91.64 (+0.007 / -0.000) |

**`scopes.statement` (scorer 10's whole scope):**

| set | schema | off P / F0.5 | seed P / F0.5 | auto P / F0.5 | auto-seed P / F0.5 |
|---|---|---|---|---|---|
| cc | 12 | 100.00 / 95.77 | 100.00 / 95.77 (+0.000 / +0.000) | 100.00 / 95.77 (+0.000 / +0.000) | 100.00 / 95.77 (+0.000 / +0.000) |
| esbuild-app | 12 | 96.33 / 89.39 | 96.37 / 89.42 (+0.047 / +0.032) | 96.33 / 89.39 (+0.000 / +0.000) | 96.37 / 89.42 (+0.047 / +0.032) |
| heldout | 12 | 99.31 / 90.87 | 99.31 / 90.87 (+0.000 / +0.000) | 99.31 / 90.87 (+0.000 / +0.000) | 99.31 / 90.87 (+0.000 / +0.000) |
| wrangler | 12 | 96.45 / 56.12 | 96.45 / 56.12 (+0.000 / +0.000) | 98.23 / 69.44 (+1.779 / +13.326) | 98.23 / 69.44 (+1.779 / +13.326) |
| apps-esbuild | 12 | 99.79 / 98.23 | 99.88 / 98.30 (+0.096 / +0.074) | 99.79 / 98.23 (+0.000 / +0.000) | 99.88 / 98.30 (+0.096 / +0.074) |
| apps-rollup | 12 | 99.09 / 94.38 | 99.11 / 94.40 (+0.020 / +0.014) | 99.09 / 94.38 (+0.000 / +0.000) | 99.11 / 94.40 (+0.020 / +0.014) |
| apps-webpack | 12 | 99.58 / 92.98 | 99.60 / 92.99 (+0.018 / +0.013) | 99.58 / 92.98 (+0.000 / +0.000) | 99.60 / 92.99 (+0.018 / +0.013) |
| dev-rollup | 12 | 99.87 / 94.43 | 99.87 / 94.43 (+0.000 / +0.000) | 99.87 / 94.43 (+0.000 / +0.000) | 99.87 / 94.43 (+0.000 / +0.000) |
| dev-webpack | 12 | 99.64 / 89.22 | 99.64 / 89.22 (+0.000 / +0.000) | 99.64 / 89.22 (+0.000 / +0.000) | 99.64 / 89.22 (+0.000 / +0.000) |
| pop-bun | 12 | 98.91 / 96.00 | 98.91 / 96.00 (+0.000 / +0.000) | 98.91 / 96.00 (+0.000 / +0.000) | 98.91 / 96.00 (+0.000 / +0.000) |
| pop-esbuild | 12 | 99.02 / 96.92 | 99.05 / 96.94 (+0.026 / +0.020) | 99.02 / 96.92 (+0.000 / +0.000) | 99.05 / 96.94 (+0.026 / +0.020) |
| remin-esbuild | 12 | 99.04 / 94.02 | 99.35 / 94.24 (+0.301 / +0.217) | 99.05 / 94.13 (+0.004 / +0.106) | 99.35 / 94.35 (+0.304 / +0.323) |
| remin-terser | 12 | 99.82 / 88.40 | 99.82 / 88.40 (+0.000 / +0.000) | 99.74 / 96.14 (-0.083 / +7.741) | 99.90 / 96.26 (+0.079 / +7.862) |
| test-bun | 12 | 99.33 / 92.90 | 99.33 / 92.90 (+0.000 / +0.000) | 99.33 / 92.90 (+0.000 / +0.000) | 99.33 / 92.90 (+0.000 / +0.000) |
| test-esbuild | 12 | 99.12 / 95.37 | 99.16 / 95.40 (+0.042 / +0.031) | 99.12 / 95.37 (+0.000 / +0.000) | 99.16 / 95.40 (+0.042 / +0.031) |
| test-rollup | 12 | 98.96 / 92.21 | 98.97 / 92.21 (+0.005 / +0.003) | 98.96 / 92.21 (+0.000 / +0.000) | 98.97 / 92.21 (+0.005 / +0.003) |
| test-webpack | 12 | 99.48 / 92.49 | 99.48 / 92.48 (+0.007 / -0.001) | 99.48 / 92.49 (+0.000 / +0.000) | 99.48 / 92.48 (+0.007 / -0.001) |
| test2-bun | 12 | 98.54 / 93.99 | 98.54 / 93.99 (+0.000 / +0.000) | 98.54 / 93.99 (+0.000 / +0.000) | 98.54 / 93.99 (+0.000 / +0.000) |
| test2-esbuild | 12 | 99.44 / 93.59 | 99.58 / 93.69 (+0.140 / +0.099) | 99.44 / 93.59 (+0.000 / +0.000) | 99.58 / 93.69 (+0.140 / +0.099) |
| test2-rollup | 12 | 96.35 / 91.35 | 96.36 / 91.36 (+0.004 / +0.003) | 96.35 / 91.35 (+0.000 / +0.000) | 96.36 / 91.36 (+0.004 / +0.003) |
| test2-webpack | 12 | 99.27 / 91.64 | 99.28 / 91.64 (+0.007 / -0.000) | 99.27 / 91.64 (+0.000 / +0.000) | 99.28 / 91.64 (+0.007 / -0.000) |

- **auto+seed vs seed:** identical on every set except wrangler, remin-esbuild and remin-terser, where P and F0.5
  are both higher.
- **auto alone vs off:** remin-terser P falls (-0.083); that is the tslib class `seed` removes.
- **Verdict.** Flipping `auto` is clean on every set once `seed` is on, under scorer 12, on both scopes. Flipping
  the pair against off costs only test-webpack / test2-webpack F0.5 -0.001 / -0.000. That comes from `seed`, which
  drops about 25 KB of right seeds in webpack bundles. Not diagnosed: test2 is never diagnosed.

## 4. Reproduce

```bash
S=experiments/129-helpers-copies; PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
# B: the index with the helper table (merge-only, k2-all-r5's store and plan, under the index-build lock)
flock /work/index-build.lock /work/exp129-data/merge.sh            # -> /work/exp129-data/ix/k2-all-r5h
for c in dev apps bundlers pop remin test test2; do                  # label prefix: <c>-, none for dev
  npm run eval -- library <c>-exp129-seed --index /work/exp129-data/ix/k2-all-r5h --rule structural --corpus $c \
    --commit 096d89e08 --pipeline-set library.discovery.helper_prints=seed
done   # likewise =credit; exp129-auto-{seed,credit} add terser_slices=auto, terser_share=0.2
# final labels (scorer 12): the same on the recommended index with the helper table, prefix exp129o-
flock /work/index-build.lock /work/exp129-data/merge130.sh       # exp130's store, --sponge-regions origin -> k2-all-r5-origin-h
#   npm run eval -- library <c>-exp129o-{seed,auto,auto-seed} --index /work/exp129-data/ix/k2-all-r5-origin-h ...
# A: rule `patched` (scorer 12 origin layer), per corpus
$PY -I $S/scorer12/origins12.py verify --corpus <c>   # patched off == the frozen scorer 10 layer
$PY -I $S/scorer12/origins12.py build --corpus <c>
# A: the copies layer v2 (units: a default run + a factory_bodies run per corpus, listed below)
$PY -I $S/copies2.py hashes /work/exp129-data/hashes.pkl dev=exp127-off,exp128-fb2 apps=...
$PY -I $S/copies2.py rows /work/exp105-data/ix/step4-wr /work/exp129-data/ix/k2-all-r5h /work/exp129-data/ix/k2-all-r5-origin-h hashes.pkl rows.pkl
$PY -I $S/copies2.py derive rows.pkl exp127-off,exp128-fb2 --corpus dev            # per corpus
$PY -I $S/verify_s11.py <label> exp096-score/11 --corpus <c>                    # before the re-score
$PY -I $S/ledger12.py <labels> --corpus <c> --json ledger-<c>.json
$PY -I $S/handcheck_draw.py 129 20 <ledger rows>...
python3 -I $S/compare.py; python3 -I $S/compare.py final; python3 -I $S/compare.py final statement
```

Units per corpus: dev `exp127-off,exp128-fb2`; apps `apps-exp127-off,exp128-fb2-apps`; bundlers
`bundlers-exp127-off,bundlers-exp129-fbunits`; pop `pop-exp127-off,pop-exp128-fb`; remin
`remin-exp127-off,remin-exp129-fbunits`; test `test-exp127-off,test-exp129-fbunits`; test2
`test2-exp124-r5,test2-exp129-fbunits`. The `*-exp129-fbunits` runs are exp128's binary with
`factory_bodies = true`, used for their units only; they are not committed. Data: `/work/exp129-data`.
