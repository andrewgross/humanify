# exp172 — a CommonJS app's own modules stay out of vendor/

STATUS: BUILT on branch `exp172-cjs-app-placement` (off main `dba4a0e10`,
not yet rebased onto exp171). In names mode on bun-dev, **345,776 of the
389,273 app bytes** that sat inside extracted CommonJS factories now leave
`vendor/` (recall 0.888), and **no library module moves** (0
library-majority factories; the 410 "library" bytes are 22–25-byte
source-map slivers at the start of 18 app modules). In default mode (no
index) only the entries move: 24,291 app bytes, 0 library bytes. Claude
Code moves 0 factories (warm neutrality below). Gate 13/13.

Andrew's decision: keep a CommonJS app's own modules out of `vendor/`.
Bun and esbuild wrap every CommonJS module as `var X = __commonJS(…)`,
app or library, and the unpack wrote every wrapped factory to `vendor/`.
exp169 phase 0 found six bun-dev apps (ts-json-schema-generator, ts-patch,
doctoc, cz-customizable, @umijs/fabric, babel-istanbul) living 100%
inside factories, 388 KB of truth-labelled `<self>` in `vendor/`.

## Phase 0: which signals separate app modules from libraries

`census.mjs` parses each bun-dev bundle (exp166's names-mode stub runs
give the vendored factories' raw spans and the index's claims), builds the
factory reference graph and joins the source-map truth: 38,690 factories in
71 judged bundles, 437 of them app-majority (389,008 bytes).
`census_signals.py` scores each signal (an app module = `<self>`/`<app>`
truth majority):

| signal                                     |  tp |     fp |  fn | tp bytes |    fp bytes | fn bytes | precision (bytes) | recall (bytes) |
| ------------------------------------------ | --: | -----: | --: | -------: | ----------: | -------: | ----------------: | -------------: |
| index silent (no claim, no package folder) |  59 |  2,928 | 378 |   71,755 |  20,271,592 |  317,253 |            0.0035 |         0.1845 |
| own package claimed (truth's own package)  | 371 |      2 |  66 |  313,695 |       4,440 |   75,313 |            0.9860 |         0.8064 |
| own package claimed (derived, gated)       | 368 |      0 |  69 |  311,647 |           0 |   77,361 |            1.0000 |         0.8011 |
| reachable from the app text                | 437 | 38,223 |   0 |  389,008 | 163,451,291 |        0 |            0.0024 |         1.0000 |
| reachable from the app text, gated         | 430 |  1,318 |   7 |  388,471 |  10,313,982 |      537 |            0.0363 |         0.9986 |
| referenced by the app text (roots)         |  16 |    522 | 421 |   16,201 |  23,887,013 |  372,807 |            0.0007 |         0.0416 |
| roots, gated (the entry)                   |  13 |      0 | 424 |   16,041 |           0 |  372,967 |            1.0000 |         0.0412 |
| no banner comment                          | 437 | 37,843 |   0 |  389,008 | 133,990,044 |        0 |            0.0029 |         1.0000 |
| bundle order (span of the app factories)   | 437 |    556 |   0 |  389,008 |   1,127,188 |        0 |            0.2566 |         1.0000 |
| **combined rule (Python proxy)**           | 397 |      0 |  40 |  346,186 |           0 |   42,822 |        **1.0000** |     **0.8899** |

"Gated": the app text outside the factories is only glue (helper
declarations, `module.exports = {}`, bare calls of factories), so the
called factories are the app's wrapped entry. 14 bundles pass: the 12
CommonJS-app bundles and three whose only factories are text assets.

What each signal says:

- **The index on its own cannot tell.** Silence is not app evidence: most
  unclaimed factories are libraries the index does not know.
- **What names the app's package folder** (the 313 KB exp169 found under
  `vendor/<app>/`): the index's claims and placement. bun-dev's apps are
  published npm packages, so the index knows them. A private app is not in
  the index; its own-package signal is then empty and only the entry and
  the modules above it move.
- **The require graph alone is useless** (163 MB of library is reachable
  from the app text), and **gated** it still sweeps in 10 MB: everything
  an app requires is reachable from it. A library never requires app code,
  though, so the graph is a sound filter in the other direction: whatever
  a library reaches is library, and whatever requires an app module is
  app.
- **The gate alone is exact but small**: the entry factories, 16 KB.
- **Bundle order does not cluster**: Bun emits modules depth-first, and an
  app's modules interleave with the libraries they pull in.
- **esbuild's source-path key**: not measurable on bun-dev (Bun keeps
  none). The e2e fixtures stand their "vendored" dependencies in as local
  `src/vendor/*.cjs` files, so a key outside `node_modules/` cannot be read
  as app evidence on its own; the rule uses only `node_modules/` keys, as
  library evidence.
- **Downward inference was measured and refused**: accepting the entry's
  unattributed children too adds 37 KB of app at the cost of 45 KB of
  library (tslib, commander and json5 entry stubs, find-config,
  anchor-markdown-header and its deps).

Claude Code: its app text is megabytes of ES-module code, and factory
bodies read app-scope bindings (finding #51/#60), so the gate fails and
nothing is examined further.

## The rule (one owner: `modules::app_factories`)

Asked once by the unpack (`unpack::bun::keep_app_factories`), after the
identities and the names-mode placement, before any name is chosen:

1. **Library evidence**: a package the index claims on the module's
   statements or places it under, a package its declared identity names
   (banner, inlined path, inlined `package.json`, …), a `node_modules/`
   source path. Everything a library module requires is library.
2. **Entry**: the factories the app text runs, when that text is only
   glue (`entry_factories`) and no factory body reaches an app-scope
   binding.
3. **Own package**: a package whose modules and the entry's unattributed
   modules require each other, or whose inlined `package.json` the entry's
   unattributed modules require. Refused when any other package's module
   requires one of its modules.
4. **Accept** the entry and the own-package modules reachable from it,
   then every reachable module that requires an accepted one (upward
   closure). Nothing downward.
5. **Placement soundness**: refuse any accepted module a vendor file
   requires, to a fixpoint.

Accepted factories stay in the app text. The era names them (they never
reach exp169's vendor-body pass, so nothing is named twice); the split's
fresh grouping groups them under `src/` — its vendor bucket, which used to
take every factory statement left in the app, skips the ones the owner
accepted (`assign_clustered`'s `app_factories`, resolved by raw name
through the fresh text, as the capture records are). Runtime references to
vendor modules keep their stable `lib_<hash>` identifiers, and the
manifest simply lists fewer factories.

Tests, each watched red first: `app_factories_test.rs` (12: the decision
and the gate), `unpack_test::a_commonjs_apps_entry_and_its_own_package_json_stay_in_the_app`,
`cluster_test::an_app_factory_statement_is_grouped_with_the_app`,
`stable_split_test::the_apps_own_factory_statements_are_grouped_with_the_app`,
`e2e-known-gaps.test.ts` `placementFailures`, and the new e2e fixture
`test/e2e/fixtures/bun-cjs-app` (a minified Bun CJS build of a 54-module
app with one library; `expect.placement` requires the app modules in
`src/` and the library in `vendor/`; on main's binary it fails with every
app module in `vendor/`). The frozen cutter/graph/hash pins enumerate the
e2e fixtures; they hold with the new fixture left out, so it is excluded
from them (`ADDED_AFTER_THE_PINS`).

## Phase 2: measured on the binary

### Gate table

| check                                        | result                                                                                                                                         |
| -------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `npm run check`                              | **ALL 13 STAGES PASSED** (at `561af57cf`'s tree)                                                                                               |
| e2e `bun-cjs-app` (new)                      | passes; red on main's binary (55 vendor files, 0 app in src/)                                                                                  |
| neutrality, Claude Code, default mode (warm) | **NEUTRAL** vs `dba4a0e10` (2.1.85→2.1.86): 0 files / 0 lines, cache +0 / +0, exit 0 / 0                                                       |
| neutrality, Claude Code, names mode (warm)   | **NEUTRAL** vs `dba4a0e10` (`library.matching=names`, current index): 0 files / 0 lines, cache candidate +15,105 / baseline **+0**, exit 0 / 0 |
| bun-dev stub runs                            | 90 bundles; the same 10 fail on main and here (names mode)                                                                                     |

### Results table (bun-dev, source-map truth; `score.py`)

| KPI                                      | main (names) | **exp172 names** | **exp172 default** |
| ---------------------------------------- | -----------: | ---------------: | -----------------: |
| factories kept in the app as app modules |            0 |              397 |                 15 |
| app bytes leaving vendor/ (of 389,273)   |            0 |      **345,776** |             24,291 |
| recall (bytes)                           |        0.000 |        **0.888** |              0.062 |
| library-majority factories moved         |            0 |            **0** |                  0 |
| library bytes moved (all slivers, below) |            0 |              410 |                  0 |
| precision (bytes)                        |            — |        **0.999** |              1.000 |

Per surface (exp170's app / vendor split): every moved byte goes from the
vendor surface to the app surface; nothing crosses the other way, and
Claude Code moves nothing on any surface (neutrality).

**The 410 library bytes, itemized**: 18 factories (9 per target), all
app-majority modules of ts-json-schema-generator and ts-patch, each
carrying 22–25 bytes the source map attributes to the previous module
(`typescript`, `json5`, `normalize-path`, `minimist`, `resolve`, `semver`,
`chalk`, `global-prefix`, `strip-ansi`): the wrapper's opening
`var X=h(function(…){` sits in the previous module's mapping. No library
code moved.

**Per app (names mode, app bytes moved / in factories, per target):**

| app                      | moved / total     | why                                                                                                                            |
| ------------------------ | ----------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| ts-json-schema-generator | 119,291 / 119,343 | own package from its `package.json` (the entry's main module requires it); 154 of 171 factories move                           |
| ts-patch                 | 47,442 / 48,745   | own package by mutual require; 42 of 125 factories                                                                             |
| doctoc                   | 5,496 / 13,413    | the entry only: nothing ties its other four modules to the app (unclaimed, no package.json)                                    |
| babel-istanbul           | 759 / 4,537       | the entry only: its modules are claimed as `istanbul` (it is a fork), library evidence                                         |
| cz-customizable          | 266 / 6,815       | the entry only                                                                                                                 |
| @umijs/fabric            | 0 / 3,726         | its entry inlines `__dirname = "…/node_modules/@umijs/fabric"` (the corpus installs the app as a dependency): library evidence |

### The six apps' trees, before and after (names mode, files / bytes)

| bundle                                   | src/ before | src/ after   | vendor/ before  | vendor/ after   | other before | other after |
| ---------------------------------------- | ----------- | ------------ | --------------- | --------------- | ------------ | ----------- |
| bun-cli_babel-istanbul-f3b1f1            | 0 / 0       | 0 / 0        | 0 / 0           | 0 / 0           | 1 / 50,481   | 1 / 50,481  |
| bun-cli_cz-customizable-36f649           | 1 / 302     | 1 / 971      | 317 / 1,461,928 | 316 / 1,461,204 | 4 / 5,304    | 4 / 5,304   |
| bun-cli_doctoc-fa09a2                    | 1 / 302     | 1 / 10,797   | 195 / 710,103   | 194 / 700,041   | 4 / 5,136    | 4 / 5,136   |
| bun-cli_ts-json-schema-generator-1165fe  | 1 / 303     | 23 / 263,753 | 171 / 7,232,987 | 17 / 6,959,082  | 4 / 5,342    | 4 / 6,329   |
| bun-cli_ts-patch-3ab5bd                  | 1 / 1,697   | 8 / 111,701  | 125 / 308,540   | 83 / 199,374    | 4 / 5,305    | 4 / 5,448   |
| bun-cli_umijs_fabric-f601b5              | 1 / 316     | 1 / 316      | 61 / 223,586    | 61 / 223,586    | 4 / 5,136    | 4 / 5,136   |
| node-cli_babel-istanbul-f3b1f1           | 0 / 0       | 0 / 0        | 10 / 44,025     | 9 / 42,181      | 1 / 173      | 1 / 2,042   |
| node-cli_cz-customizable-36f649          | 0 / 0       | 0 / 0        | 317 / 1,165,468 | 316 / 1,165,003 | 1 / 173      | 1 / 663     |
| node-cli_doctoc-fa09a2                   | 0 / 0       | 0 / 0        | 195 / 652,835   | 194 / 643,087   | 1 / 173      | 1 / 9,946   |
| node-cli_ts-json-schema-generator-1165fe | 0 / 0       | 0 / 0        | 171 / 7,176,421 | 17 / 6,956,547  | 1 / 174      | 1 / 223,317 |
| node-cli_ts-patch-3ab5bd                 | 0 / 0       | 0 / 0        | 125 / 282,258   | 83 / 183,864    | 1 / 1,435    | 1 / 100,686 |
| node-cli_umijs_fabric-f601b5             | 0 / 0       | 0 / 0        | 61 / 214,586    | 61 / 214,586    | 1 / 173      | 1 / 173     |

(bun-cli_babel-istanbul writes one unsplit file in both runs; it is not among the 71 judged bundles.)

The `--target node` builds have no wrapper function, so the split does
not run (spec I25, a known gap): their app modules land in `runtime.js`
("other"), like every app statement of such a bundle. The `--target bun`
builds split, and the app modules land under `src/`.

## What this does not look at (rule 8)

- **Real CommonJS apps built with `--format=cjs` whose bin has code.** Bun
  leaves a CommonJS entry unwrapped there; its code sits in the app text,
  the gate fails, and nothing moves. A bin that only requires the app
  (`require("../lib/cli")`) passes, as does Bun's default ESM format (it
  wraps the entry: `export default require_cli()`). The corpus's synthetic
  70-byte entries all pass.
- **Private apps.** Without the index knowing the app's package, only the
  entry, the app's `package.json` (when read) and the modules above them
  move (default mode on bun-dev: recall 0.062).
- **Downward recall** (doctoc, cz-customizable): refused by design until a
  signal separates an app's leaf modules from unindexed libraries.
- **The bun-dev runs use the stub LLM** and were not repeated: placement is
  decided before any ask.
- **esbuild**: no esbuild corpus with truth; only the e2e fixtures (all
  pass; `humanify unpack` writes the same file count as main on every
  fixture but `bun-cjs-app`).
- No cross-version eval: on Claude Code the change is byte-neutral, so
  every eval KPI is unchanged by construction.

## Follow-ups

- A CommonJS entry left unwrapped in the app text (`--format=cjs`) needs a
  way to tell the entry's own requires apart from its libraries' before it
  can seed anything.
- A prior from before this change vendored these modules; the first hop
  after it sees them as new app code (steady state unaffected).
