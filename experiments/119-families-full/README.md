# exp119: metadata families over every package of the default index (scorer 8)

## STATUS (2026-10-15)

- **DONE on branch `exp119-families-full`** (off main `f581d036`; main `34b7bd63`, shadow_ties on, merged in). NOT merged.
- **What changed:** `crates/humanify-core/src/libindex/families-derived.json` was re-derived with exp104's
  `derive_families.py`, rules unchanged. The universe is now every package of the default index k2-all (12,122)
  plus the earlier universe (11,410): 17,316 names in total. The file is the ONE links file read by the matcher
  (`derived_families`, on by default) and by the scorer's `family()`.
  - Links: 3,367 -> **5,562** (repo 3,288 -> 5,362, rename 44 -> 119, fork 35 -> 81). Every pair the old file
    joined is still joined (checked by union-find over the new links), so the new file is a strict superset.
  - `@redocly/ajv ~ ajv` is now a fork link (`github.com/ajv-validator/ajv -> github.com/redocly/ajv`).
  - Test first: `families_test.rs`, `the_shipped_derived_links_cover_the_default_index_s_packages`. It FAILED on
    the old file (red) and passes on the new one (green).
- **Scorer 8** (`exp096-score/8`): only the links file changed. All 223 committed labels were re-scored once
  (dev 78, apps 39, bundlers 15, pop 14, test 54, test2 23).
- **Truth ledger, both directions, on every label:** **right -> wrong = 0 bytes everywhere.** Every relabel is
  wrong -> right. Dev (78 labels: held-out, CC, task-master, wrangler) moved 0 bytes. The moves are on the
  webpack and rollup sets (forks such as @redocly/ajv, @eslint-community/eslint-utils) and on pop-bun.
- **Hand check:** 20 byte-weighted relabelled units (seed 119, dev corpora only). **20/20 correct** as family
  verdicts. One of them is generic TypeScript helper code inside the forked package.
- **Matcher (main's defaults incl. shadow_ties, k2-all), before -> after the new file:** held-out F0.5 90.41 -> 90.42,
  P 99.24 =. With shadow_ties off (f581d036's defaults): 90.35 -> 90.35.
  Recall rises slightly on webpack and rollup. Precision drops by at most 0.07 points, in two places:
  - pop-esbuild 93.59 -> 93.52 (+68 KB wrong). This is a truth artifact, diagnosed below.
  - apps-webpack +4.6 KB wrong, a fork that is missing from the universe. P is 98.61 either way.

  test-esbuild adds 2.6 KB of wrong bytes and P stays 96.42. App claimed is 0 everywhere.
- **`shadow_ties` (ON in main's defaults since `34b7bd63`, Andrew's call; merged in and kept on).** Main's new
  defaults are this branch's `*exp119-k2all-st*` config. The merged HEAD with no `--pipeline-set` reproduces those
  labels' set summaries exactly (dev and bundlers: `exp119-head*`). **Does the new families file change what
  shadow_ties does? On the fork pairs, yes: families now catch them on their own.** On scorer 7's links,
  shadow_ties moved every fork claim to the original's name. On apps, `@redocly/ajv` 293,508 B,
  `@cypress/request` 17,251 B, `@supabase/node-fetch` 19,378 B, `@soda/*`, `@apollo/protobufjs` and
  `@eslint-community/eslint-utils` all went to 0. On the new links, a fork and its original are ONE family, so they
  are no longer rivals and shadow_ties leaves those claims alone (`@redocly/ajv` stays at 297,992 B with or
  without it). The scorer judges them right through the same links. Everywhere else shadow_ties' effect is
  unchanged: its wrong-byte reduction is the same to the byte on every set (held-out -17,649 B, apps-rollup
  -297,556 B, test2-esbuild still +2,685 B). Its F0.5 gain on dev-rollup (+0.24 on the old links) is now +0.00,
  because the families give the same recall without it. One side effect: the matcher now NAMES the fork
  (`@redocly/ajv`) where shadow_ties used to name the original (`ajv`). The verdict is right at family level, but
  the package name is less specific.
- **Gate:** ALL 13 STAGES PASSED, bare, at `37687fc8` and again on the final tree (see the commit log).
- **Neutrality, warm.** Pair 2.1.215 -> 2.1.216, named explicitly. Flags `--priors /work/exp050-cold --cache
  /work/neutrality-cache`. Run under `flock /work/neutrality.lock`, each time from a clean detached worktree of the
  candidate commit.
  - **vs main `34b7bd63` (shadow_ties on), candidate merge `02f783a6`:** default mode **NEUTRAL, 0 files / 0 lines,
    cache +0 / +0, exit 0 / 0**. Report mode (`--pipeline-set library.matching=report library.index=/work/exp113-data/ix/k2-all`):
    **NEUTRAL, 0 / 0, +0 / +0, exit 0 / 0**. Only `.humanify/library-report.json` differs, a diagnostics-only artifact.
  - **vs main `f581d036`, candidate `37687fc8`:** the same result in both modes. In report mode, the
    library-report diff on Claude Code 2.1.216 is one 321-byte unit of `@wolfy1339/lru-cache` (a GitHub fork of
    lru-cache) that is no longer vetoed. lru-cache's grown and queued bytes move by about 350 B. The scored `cc` set
    is unchanged.

## How the file was rebuilt

1. **Metadata (cache first).** `/work/exp106-data/registry` (exp106's superset, 880 files more) was copied into
   `/work/exp104-data/registry` without overwriting anything. That reproduces the committed file byte for byte
   from `/work/exp106-data/universe-union.json` (after prettier). Then the 5,907 k2-all names with no cached
   `@latest` manifest were fetched with exp104's `fetch_metadata.py` (8 threads). One failed:
   `@js-sdsl/.build-data`, which is not a package.
2. **GitHub fork data.** 106 fork candidates had no cached repository record. `fetch_github.py` (new here) fetches
   them sequentially, unauthenticated, keeping only `full_name`, `fork` and `parent`. On a rate-limit refusal it
   waits for the API's advertised reset. After this, `forkCandidatesWithoutGithubData` is empty.
3. **Derive.** `python3 -I experiments/104-truth-audit/derive_families.py /work/exp104-data/registry
   /work/exp119-data/universe-k2all.json crates/humanify-core/src/libindex/families-derived.json`, then prettier.

The rules are exp104's, unchanged:

- **repo:** same normalised repository URL. The directory is NOT part of the key, as before.
- **rename:** a "renamed / moved to X" deprecation.
- **fork:** a scoped `@x/N`, same repository name as `N`, recorded as a fork of `N`'s repository by GitHub.

The judgement is pairwise, with no folding through scope families. The largest new repository groups are real
monorepos: react-spectrum (`@internationalized/*` with `@react-aria/*`), jest (`@jest/*` with `jest-*`),
oxc, cssnano and similar. `@types/*` (DefinitelyTyped) is one key, as before.

**Not rebuilt:** the frozen copies layers (`copies*.json`) still carry the families sha they were built with.
Their dependency test reads families, so rebuilding them would be a second truth change. Scorer 8 changes
`family()` only.

## Truth ledger (scorer 7 -> 8; `ledger.py`; bytes)

`ledger.py` labels every claimed unit exactly as `score.py` does, then judges the claim twice: once with the old
links (scorer 7's file) and once with the new ones. A changed verdict is charged to the new link kind that alone
joins the pair. Over all 223 labels, **no transition other than WRONG -> GOOD occurs**. GOOD -> WRONG,
GOOD -> UNJUDGED and the rest are all 0. Labels with any relabel: dev 0/78, apps 37/39, bundlers 4/15,
pop 13/14, test 25/54, test2 23/23. Per label and set: `ledger-{dev,apps,bundlers,pop,test,test2}.json`. test
and test2 hold per-set aggregates only.

Shipped-default labels (`*exp117-k2all-cpfdmb*`), wrong -> right bytes (right -> wrong is 0 on every row):

| corpus   | set           | wrong -> right | by link kind                                 |
| -------- | ------------- | -------------- | -------------------------------------------- |
| dev      | all 4 sets    | 0              | -                                            |
| apps     | apps-esbuild  | 114            | repo 114                                     |
| apps     | apps-rollup   | 170,422        | fork 114,681, repo 55,741                    |
| apps     | apps-webpack  | 125,605        | fork 118,899, repo 6,706                     |
| bundlers | dev-rollup    | 22,145         | fork 22,145 (@redocly/ajv)                   |
| bundlers | dev-webpack   | 20,904         | fork 20,904 (@redocly/ajv)                   |
| pop      | pop-bun       | 35,333         | repo 35,333                                  |
| pop      | pop-esbuild   | 0              | -                                            |
| test     | test-bun      | 43,072         | repo                                         |
| test     | test-esbuild  | 7,837          | repo                                         |
| test     | test-rollup   | 159,389        | fork 101,934, repo 57,455                    |
| test     | test-webpack  | 257,578        | fork 162,923, repo 90,638, rename 4,017      |
| test2    | test2-bun     | 68             | repo                                         |
| test2    | test2-esbuild | 541            | repo                                         |
| test2    | test2-rollup  | 370,733        | fork 360,760, repo 9,973                     |
| test2    | test2-webpack | 207,716        | fork 174,369, repo 33,347                    |

`rescore_diff.py` (exp107) over all labels agrees with the ledger. On dev, only `schema` and
`config.familiesSha256` moved, with one exception. One card of `exp113-own-k2` (held-out codex) gains
`copyBytes` 0 -> 24,010. That committed card was stale, which is not a family effect, and no verdict moved.
On apps, bundlers, pop, test and test2, the claim-verdict fields moved, and the exploratory exact-name and miss
columns that follow `family()` moved with them.

The ledger matches the cards. For example, dev-rollup's wrong-family bytes go from 24,259 to 2,114 (-22,145),
and test2-rollup's from 2,339,455 to 1,968,722 (-370,733).

Scorer 7 -> 8 on the same labels (P / R / F0.5, %; the matcher did not run again):

| set           | P              | R              | F0.5           |
| ------------- | -------------- | -------------- | -------------- |
| heldout       | 99.16 =        | 66.66 =        | 90.35 =        |
| apps-rollup   | 97.95 -> 98.18 | 76.45 -> 76.63 | 92.74 -> 92.95 |
| apps-webpack  | 97.93 -> 98.16 | 72.06 -> 72.22 | 91.37 -> 91.58 |
| dev-rollup    | 98.52 -> 99.87 | 75.61 -> 76.65 | 92.90 -> 94.17 |
| dev-webpack   | 97.80 -> 99.61 | 61.64 -> 62.78 | 87.53 -> 89.15 |
| pop-bun       | 94.84 -> 94.88 | 76.90 -> 76.93 | 90.61 -> 90.65 |
| test-bun      | 97.10 -> 97.19 | 66.31 -> 66.38 | 88.85 -> 88.93 |
| test-esbuild  | 96.34 -> 96.36 | 75.91 -> 75.92 | 91.42 -> 91.43 |
| test-rollup   | 97.40 -> 97.61 | 67.92 -> 68.07 | 89.62 -> 89.82 |
| test-webpack  | 98.80 -> 99.05 | 70.04 -> 70.21 | 91.30 -> 91.53 |
| test2-bun     | 98.62 =        | 77.65 =        | 93.57 =        |
| test2-esbuild | 99.41 -> 99.42 | 74.16 =        | 93.08 =        |
| test2-rollup  | 94.84 -> 95.66 | 72.78 -> 73.40 | 89.42 -> 90.19 |
| test2-webpack | 98.78 -> 98.96 | 68.85 -> 68.97 | 90.88 -> 91.04 |

Recall rises because a claim newly judged "family" counts as named. The denominator does not depend on `family()`.

## Hand check (`handcheck.json`; `handcheck_draw.py`, seed 119, byte-weighted, dev corpora only)

The pool is 1,467 distinct relabelled units from the dev, apps, bundlers and pop ledgers. test and test2 were not
drawn. Each unit's text was read from the frozen bundle.

| draws | claim <- truth label                                                     | verdict                                                                                                               |
| ----- | ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| 13    | `@redocly/ajv` <- `ajv`                                                  | correct: ajv's codegen, errors and compile modules. GitHub fork                                                      |
| 2     | `@eslint-community/eslint-utils` <- `eslint-utils`                       | correct: ReferenceTracker. GitHub fork                                                                                |
| 1 each | `@supabase/node-fetch` <- `node-fetch`; `@soda/friendly-errors-webpack-plugin`; `error-stack-parser` <- `@devexpress/error-stack-parser` | correct: GitHub forks, same code                                                         |
| 1     | `pouchdb-browser` <- `pouchdb`                                           | correct: one monorepo (pouchdb-find's selector matchers)                                                              |
| 1     | `@mark.probst/typescript-json-schema` <- `typescript-json-schema`        | correct as family. The unit itself is TypeScript's `__assign` / `__awaiter` helpers in the package's build (generic code) |

## Matcher: main's current defaults (shadow_ties on), before -> after the new file

"Before" is exp117's `*exp117-k2all-flip4*` labels: cpf + dmb + shadow_ties, which are main `34b7bd63`'s
defaults, re-scored under scorer 8. "After" is `*exp119-k2all-st*`, which equals this branch's merged defaults
(`exp119-head*` are identical). Both are structural, scorer 8, on k2-all. Values are P / R / F0.5 in %.

| set | before (main 34b7bd63 defaults) | after (this branch) | dF0.5 | wrong-family bytes | app |
|---|---|---|---|---|---|
| cc | 100.00 / 81.90 / 95.77 | 100.00 / 81.90 / 95.77 | +0.00 | 0 -> 0 | 0 -> 0 |
| esbuild-app | 96.25 / 65.48 / 87.98 | 96.25 / 65.50 / 87.99 | +0.01 | 125,529 -> 125,529 | 0 -> 0 |
| heldout | 99.24 / 66.69 / 90.41 | 99.24 / 66.69 / 90.42 | +0.00 | 154,754 -> 154,751 | 0 -> 0 |
| wrangler | 95.99 / 18.52 / 52.26 | 95.99 / 18.53 / 52.28 | +0.02 | 5,652 -> 5,652 | 0 -> 0 |
| apps-esbuild | 98.73 / 89.71 / 96.79 | 98.73 / 89.71 / 96.79 | +0.00 | 295,428 -> 295,428 | 0 -> 0 |
| apps-rollup | 98.57 / 77.01 / 93.34 | 98.57 / 77.12 / 93.38 | +0.03 | 1,095,712 -> 1,095,684 | 0 -> 0 |
| apps-webpack | 98.61 / 72.55 / 92.00 | 98.61 / 72.75 / 92.06 | +0.06 | 760,061 -> 764,663 | 0 -> 0 |
| dev-rollup | 99.87 / 77.44 / 94.40 | 99.87 / 77.53 / 94.43 | +0.03 | 2,114 -> 2,114 | 0 -> 0 |
| dev-webpack | 99.64 / 62.63 / 89.11 | 99.64 / 62.89 / 89.22 | +0.11 | 4,139 -> 4,139 | 0 -> 0 |
| pop-bun | 95.07 / 77.06 / 90.83 | 95.08 / 77.13 / 90.85 | +0.02 | 4,040,238 -> 4,040,238 | 0 -> 0 |
| pop-esbuild | 93.59 / 79.54 / 90.40 | 93.52 / 79.54 / 90.34 | -0.05 | 5,532,133 -> 5,600,477 | 0 -> 0 |
| test-bun | 97.23 / 66.36 / 88.95 | 97.23 / 66.39 / 88.96 | +0.01 | 1,235,745 -> 1,235,745 | 0 -> 0 |
| test-esbuild | 96.42 / 75.93 / 91.48 | 96.42 / 75.96 / 91.49 | +0.00 | 1,944,386 -> 1,946,978 | 0 -> 0 |
| test-rollup | 97.89 / 68.25 / 90.06 | 97.89 / 68.39 / 90.12 | +0.05 | 1,607,380 -> 1,607,380 | 0 -> 0 |
| test-webpack | 99.18 / 70.30 / 91.65 | 99.19 / 70.51 / 91.73 | +0.07 | 865,283 -> 865,443 | 0 -> 0 |
| test2-bun | 98.71 / 77.70 / 93.64 | 98.71 / 77.70 / 93.64 | +0.00 | 423,750 -> 423,750 | 0 -> 0 |
| test2-esbuild | 99.41 / 74.21 / 93.09 | 99.41 / 74.21 / 93.09 | +0.00 | 190,602 -> 190,602 | 0 -> 0 |
| test2-rollup | 96.15 / 74.13 / 90.76 | 96.18 / 74.74 / 90.96 | +0.20 | 1,756,319 -> 1,756,319 | 0 -> 0 |
| test2-webpack | 99.13 / 69.13 / 91.22 | 99.14 / 69.22 / 91.25 | +0.03 | 1,029,629 -> 1,029,629 | 0 -> 0 |

F0.5 is up or equal on every set except pop-esbuild (-0.05). That set, apps-webpack and test-esbuild are the only
sets that add wrong bytes: +68,344, +4,602 and +2,592 B, the same as with shadow_ties off. The two dev cases are
the scorer blind spots diagnosed below.

## Matcher with shadow_ties off (main `f581d036`'s defaults), before -> after the new file

"Before" is exp117's `*exp117-k2all-cpfdmb*` labels: main `f581d036`'s defaults, re-scored under scorer 8. "After"
is `*exp119-k2all-def*`: this branch's defaults with no `--pipeline-set`. Both are structural, scorer 8. Values are
P / R / F0.5 in %.

| corpus   | set           | before                 | after                  | dF0.5 | wrong-family bytes     | app |
| -------- | ------------- | ---------------------- | ---------------------- | ----- | ---------------------- | --- |
| dev      | cc            | 100.00 / 81.90 / 95.77 | 100.00 / 81.90 / 95.77 | +0.00 | 0 -> 0                 | 0   |
| dev      | esbuild-app   | 96.21 / 65.45 / 87.94  | 96.21 / 65.48 / 87.95  | +0.01 | 126,841 -> 126,841     | 0   |
| dev      | heldout       | 99.16 / 66.66 / 90.35  | 99.16 / 66.67 / 90.35  | +0.00 | 172,403 -> 172,400     | 0   |
| dev      | wrangler      | 95.99 / 18.52 / 52.26  | 95.99 / 18.53 / 52.28  | +0.02 | 5,652 -> 5,652         | 0   |
| apps     | apps-esbuild  | 98.64 / 89.69 / 96.71  | 98.64 / 89.69 / 96.71  | +0.00 | 316,158 -> 316,158     | 0   |
| apps     | apps-rollup   | 98.18 / 76.63 / 92.95  | 98.18 / 76.80 / 93.00  | +0.06 | 1,393,268 -> 1,393,240 | 0   |
| apps     | apps-webpack  | 98.16 / 72.22 / 91.58  | 98.16 / 72.42 / 91.65  | +0.06 | 1,006,556 -> 1,011,158 | 0   |
| bundlers | dev-rollup    | 99.87 / 76.65 / 94.17  | 99.87 / 77.53 / 94.43  | +0.26 | 2,114 -> 2,114         | 0   |
| bundlers | dev-webpack   | 99.61 / 62.78 / 89.15  | 99.61 / 62.87 / 89.19  | +0.04 | 4,518 -> 4,518         | 0   |
| pop      | pop-bun       | 94.88 / 76.93 / 90.65  | 94.88 / 77.00 / 90.67  | +0.02 | 4,202,272 -> 4,202,272 | 0   |
| pop      | pop-esbuild   | 93.41 / 79.40 / 90.23  | 93.34 / 79.40 / 90.17  | -0.05 | 5,683,553 -> 5,751,897 | 0   |
| test     | test-bun      | 97.19 / 66.38 / 88.93  | 97.19 / 66.37 / 88.93  | -0.00 | 1,252,036 -> 1,252,036 | 0   |
| test     | test-esbuild  | 96.36 / 75.92 / 91.43  | 96.35 / 75.94 / 91.43  | +0.00 | 1,980,016 -> 1,982,608 | 0   |
| test     | test-rollup   | 97.61 / 68.07 / 89.82  | 97.62 / 68.27 / 89.89  | +0.08 | 1,815,392 -> 1,815,350 | 0   |
| test     | test-webpack  | 99.05 / 70.21 / 91.53  | 99.05 / 70.41 / 91.60  | +0.07 | 1,009,337 -> 1,009,497 | 0   |
| test2    | test2-bun     | 98.62 / 77.65 / 93.57  | 98.62 / 77.65 / 93.57  | +0.00 | 450,855 -> 450,855     | 0   |
| test2    | test2-esbuild | 99.42 / 74.16 / 93.08  | 99.42 / 74.17 / 93.08  | +0.00 | 187,917 -> 187,917     | 0   |
| test2    | test2-rollup  | 95.66 / 73.40 / 90.19  | 95.72 / 74.41 / 90.53  | +0.34 | 1,968,722 -> 1,968,777 | 0   |
| test2    | test2-webpack | 98.96 / 68.97 / 91.04  | 98.96 / 69.07 / 91.08  | +0.04 | 1,240,202 -> 1,240,202 | 0   |

The matcher now treats more packages as one family: lookup agreement, the isolation veto, containment and the
conflict guards all use the new links. The two wrong-byte increases on dev corpora were diagnosed. Both are
scorer blind spots, not matcher errors:

- **pop-esbuild +68,344 B, one app.** The new repo link `primevue ~ @primevue/core` lets the matcher claim more of
  the app's PrimeVue code as `primevue`. The truth labels those bytes `@primevue/src`. That name comes from a
  chained source path (primevue's own map, `../../src/...`); it is no npm package, so it is in no family. The code is
  primevue's own source (`base/Base.js`, `usestyle/UseStyle.js`), so the claims are right in fact. The fix belongs
  in a truth rule (exp106's `chained`, applied to a source path that names no real package), not in this lane.
- **apps-webpack +4,602 B, one app.** The matcher now claims `@dagrejs/graphlib` where it claimed `graphlib`
  (repo link, same repository). The truth label is `@snyk/graphlib`, Snyk's fork of graphlib, which is not in
  the universe: it is neither in k2-all nor in exp104's names. Follow-up: add every DEV corpus truth label to the
  universe (apps, bundlers and pop came after exp104).
- test-esbuild +2,592 B and test-webpack +160 B: summaries only, not diagnosed.

## `shadow_ties` on the new file (`*exp119-k2all-st*` vs `*exp119-k2all-def*`)

| set           | def P / R / F0.5      | +shadow_ties P / R / F0.5 | dF0.5 | wrong-family bytes     |
| ------------- | --------------------- | ------------------------- | ----- | ---------------------- |
| heldout       | 99.16 / 66.67 / 90.35 | 99.24 / 66.69 / 90.42     | +0.07 | 172,400 -> 154,751     |
| esbuild-app   | 96.21 / 65.48 / 87.95 | 96.25 / 65.50 / 87.99     | +0.04 | 126,841 -> 125,529     |
| cc, wrangler  | =                     | =                         | +0.00 | =                      |
| apps-esbuild  | 98.64 / 89.69 / 96.71 | 98.73 / 89.71 / 96.79     | +0.07 | 316,158 -> 295,428     |
| apps-rollup   | 98.18 / 76.80 / 93.00 | 98.57 / 77.12 / 93.38     | +0.37 | 1,393,240 -> 1,095,684 |
| apps-webpack  | 98.16 / 72.42 / 91.65 | 98.61 / 72.75 / 92.06     | +0.42 | 1,011,158 -> 764,663   |
| dev-rollup    | 99.87 / 77.53 / 94.43 | 99.87 / 77.53 / 94.43     | +0.00 | 2,114 -> 2,114         |
| dev-webpack   | 99.61 / 62.87 / 89.19 | 99.64 / 62.89 / 89.22     | +0.03 | 4,518 -> 4,139         |
| pop-bun       | 94.88 / 77.00 / 90.67 | 95.08 / 77.13 / 90.85     | +0.18 | 4,202,272 -> 4,040,238 |
| pop-esbuild   | 93.34 / 79.40 / 90.17 | 93.52 / 79.54 / 90.34     | +0.17 | 5,751,897 -> 5,600,477 |
| test-bun      | 97.19 / 66.37 / 88.93 | 97.23 / 66.39 / 88.96     | +0.03 | 1,252,036 -> 1,235,745 |
| test-esbuild  | 96.35 / 75.94 / 91.43 | 96.42 / 75.96 / 91.49     | +0.05 | 1,982,608 -> 1,946,978 |
| test-rollup   | 97.62 / 68.27 / 89.89 | 97.89 / 68.39 / 90.12     | +0.22 | 1,815,350 -> 1,607,380 |
| test-webpack  | 99.05 / 70.41 / 91.60 | 99.19 / 70.51 / 91.73     | +0.13 | 1,009,497 -> 865,443   |
| test2-bun     | 98.62 / 77.65 / 93.57 | 98.71 / 77.70 / 93.64     | +0.07 | 450,855 -> 423,750     |
| test2-esbuild | 99.42 / 74.17 / 93.08 | **99.41** / 74.21 / 93.09 | +0.01 | 187,917 -> **190,602** |
| test2-rollup  | 95.72 / 74.41 / 90.53 | 96.18 / 74.74 / 90.96     | +0.43 | 1,968,777 -> 1,756,319 |
| test2-webpack | 98.96 / 69.07 / 91.08 | 99.14 / 69.22 / 91.25     | +0.17 | 1,240,202 -> 1,029,629 |

App claimed is 0 everywhere. With the new links, shadow_ties still raises F0.5 on every set, and precision on every
set except test2-esbuild (-0.01, +2,685 B). Both effects are the same as on scorer 7's links. shadow_ties is ON in
main's defaults (Andrew's call), and the merged branch keeps it on. test2 was read as a summary only, as in
exp117; nothing on it was diagnosed.

## Files

- `ledger.py`: the scorer 7 -> 8 ledger (`ledger-*.json`).
- `fetch_github.py`: the GitHub fork-data fetcher.
- `handcheck_draw.py` and `handcheck.json`: the hand-check draw and its verdicts.
- `experiments/096-library-matching-score/score.py`: `SCORER_VERSION = "exp096-score/8"`.
- Labels: `exp119-k2all-{def,st}{,-apps,-bundlers}`, `{pop,test,test2}-exp119-k2all-{def,st}`, and all 223 older
  labels re-scored.

## Reproduce

```bash
# metadata (cache first; only missing names are fetched)
python3 -I experiments/104-truth-audit/fetch_metadata.py /work/exp104-data/registry /work/exp119-data/fetch-missing.json
python3 -I experiments/104-truth-audit/derive_families.py /work/exp104-data/registry /work/exp119-data/universe-k2all.json /work/exp119-data/fam-pre-gh.json
python3 -I experiments/119-families-full/fetch_github.py /work/exp104-data/registry /work/exp119-data/fam-pre-gh.json
python3 -I experiments/104-truth-audit/derive_families.py /work/exp104-data/registry /work/exp119-data/universe-k2all.json \
  crates/humanify-core/src/libindex/families-derived.json && npx prettier --write crates/humanify-core/src/libindex/families-derived.json
# ledger (old file = scorer 7's families-derived.json), per corpus
<venv python> -I experiments/119-families-full/ledger.py /work/exp119-data/families-derived-s7.json <labels...> --corpus apps \
  --json ledger-apps.json --rows apps.jsonl
python3 -I experiments/119-families-full/handcheck_draw.py draws.json {dev,apps,bundlers,pop}.jsonl
# matcher
C="--index /work/exp113-data/ix/k2-all --rule structural --cpus 0-11 --jobs 4"
npm run eval -- library exp119-k2all-def $C            # + --corpus apps|bundlers|pop|test|test2
npm run eval -- library exp119-k2all-st $C --pipeline-set library.discovery.shadow_ties=true
# neutrality (from a clean detached worktree of the commit)
flock /work/neutrality.lock npm run eval -- neutrality f581d036 2.1.215:2.1.216 --priors /work/exp050-cold --cache /work/neutrality-cache
flock /work/neutrality.lock npm run eval -- neutrality f581d036 2.1.215:2.1.216 --priors /work/exp050-cold --cache /work/neutrality-cache \
  --pipeline-set library.matching=report --pipeline-set library.index=/work/exp113-data/ix/k2-all
```
