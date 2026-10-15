# exp112: why test precision is far below dev precision

## STATUS (2026-10-14)

- **State:** DONE on branch `exp112-test-precision` (off main `829da98f`), NOT merged.
  - Scorer bumped to `exp096-score/7`. Every committed label was re-scored once (see "Re-score").
  - One matcher key was added, default off: `library.discovery.declared_max_bytes`.
  - A new dev corpus was added: `pop` (`npm run eval -- library <label> ... --corpus pop`).
  - The bare gate passed all 13 stages. Warm neutrality against main `829da98f` is NEUTRAL 0/0, cache +0/+0.
- **The answer, totals first.** On the frozen test split, about 75% of the wrong-family bytes were not matcher
  errors. They were RIGHT claims that the truth credited to the package that shipped them. The cause was two
  copy shapes that the origin layer could not see.
  - A further ~12% are nested copies whose claims look right but carry no name signal.
  - ~7.5% are one matcher overreach: a `declared` seed on a huge statement.
  - Both shapes came from **one package family that pre-bundles a code formatter and its plugins**.
  - **Two of the 55 test apps carry about 89% of each test set's wrong-family bytes.** Each of those two apps
    installs one of two versions of that package.
  - The dev corpora never contained that shape. It is a heavy-tail sampling effect of the test population,
    not a difference in how the matcher behaves.
  - A dev set drawn from the same population (`pop`) reproduces the gap: precision 83.5 / 83.0 there,
    against test's 80.6 / 83.6.
- **TEST, k2-l12m4 index + exp106's recommended keys, structural rule. P / R / F0.5 / F1, summaries only:**

  | set          | scorer 6 (before)         | scorer 7 (truth fix)      | scorer 7 + `declared_max_bytes=65536` |
  | ------------ | ------------------------- | ------------------------- | ------------------------------------- |
  | test-bun     | 80.6 / 56.0 / 74.1 / 66.1 | 95.4 / 66.3 / 87.7 / 78.3 | **96.9 / 66.3 / 88.7 / 78.7**         |
  | test-esbuild | 83.6 / 66.2 / 79.4 / 73.9 | 95.9 / 75.9 / 91.1 / 84.7 | **96.3 / 75.9 / 91.4 / 84.9**         |

  - Wrong-family bytes fell from 8.81 / 8.94 MB to 2.07 / 2.26 MB under scorer 7, and to 1.39 / 2.03 MB with
    the cap. App claimed is 0 on every label.
  - Recall rises with scorer 7 because claims that name the copy's ORIGIN now count as named. These are the
    same claims as before; no new names were added.
  - Test is still below the 99.8% precision bar.

- **DEV, same configuration** (F0.5; scorer 6 -> 7 -> 7 + cap):

  | set                       | scorer 6 | scorer 7 | + cap    | precision 6 -> 7 -> cap |
  | ------------------------- | -------- | -------- | -------- | ----------------------- |
  | pop-bun (46 apps, new)    | 80.4     | 88.0     | **90.5** | 83.5 -> 91.3 -> 94.7    |
  | pop-esbuild               | 80.6     | 87.9     | **90.0** | 83.0 -> 90.5 -> 93.2    |
  | held-out 67               | 90.2     | 90.2     | 90.2     | 99.1, unchanged         |
  | esbuild-app, wrangler, CC | =        | =        | =        | unchanged               |
  | apps-esbuild (exp107)     | 96.6     | 96.6     | 96.6     | 98.5, unchanged         |

  - On the dev corpus and on apps-esbuild, the cap changes no card: 70/70 dev cards are identical, and the
    apps-esbuild summary is identical. Scorer 7 moves no claim verdict on dev or apps. Its ledger moves bytes
    one way only: wrong-family -> origin, on pop.

## 1. The mechanism table (aggregate)

Here is how this was measured:

- `rows.py` gives one row per claimed statement unit, judged exactly as score.py judges it.
- `evidence.py` adds the index owners of the unit's whole print.
- `mech.py` classifies each wrong-family row by reading the SHIPPER's own file (the map's `sourcesContent`) at
  the unit's mapped line.

The committed outputs are aggregates only: `mechanisms-test.json` and `mechanisms-pop.json`. Per-app rows stay
under `/work/exp112-data`.

The label is `test-exp112-k2-rec` (k2 + recommended keys), under scorer 6, the truth the campaign was reading:

| mechanism (where the shipper's own file puts the unit)                                                                                                                                                                                                | test-bun scorer 6 | scorer 7      | + cap         | test-esbuild scorer 6 | scorer 7      | + cap         |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- | ------------- | ------------- | --------------------- | ------------- | ------------- |
| **lazy body after `//#endregion`**: rolldown emits a lazily initialised module as its region (hoisted declarations) and then, after `//#endregion`, its body `__esmMin(() => {...})()`. The origin layer ended the copy at `//#endregion`. Truth gap. | 3,371,331 (38.3%) | 0             | 0             | 3,309,639 (37.0%)     | 0             | 0             |
| **nested copy**: the unit lies in a marker region naming package O, and the claim names a third package P inside O's own pre-bundled chunk.                                                                                                           | 4,982,788 (56.6%) | 1,724,241     | 1,049,148     | 4,966,814 (55.5%)     | 1,680,892     | 1,455,728     |
| ... the chunk is named after P (`<O>/dist/jiti-<hash>.mjs`) and O declares P as a build input. Truth gap.                                                                                                                                             | 3,258,547         | 0             | 0             | 3,285,922             | 0             | 0             |
| ... the claim is a `declared` seed (an inlined `package.json`) on a statement over 64 KB: the whole mixed statement is named after one manifest inside it. Matcher error.                                                                             | 671,406           | 671,406       | 0             | 662,647               | 662,647       | 0             |
| ... no name signal (tailwindcss internals and jiti inside a plugin's `v3`/`v4` chunks, editorconfig inside prettier's `index.mjs`). Claims are mostly right by content; the truth cannot see it.                                                      | 1,052,835         | 1,052,835     | 1,049,148     | 1,018,245             | 1,018,245     | 1,455,728     |
| **no marker in the shipper's file**: an unmarked copy (`_chunks/libs/<pkg>.mjs`, `_deps/<pkg>/`), a sibling shipper of one origin, or the `src` artifact below.                                                                                       | 435,537 (4.9%)    | 325,849       | 325,849       | 647,265 (7.2%)        | 558,671       | 558,671       |
| own region / before the first marker / no content: genuine matcher errors (a package named for code its region says is another's).                                                                                                                    | 18,881 (0.2%)     | 18,881        | 18,523        | 20,045 (0.2%)         | 20,045        | 20,011        |
| **total wrong-family**                                                                                                                                                                                                                                | **8,808,537**     | **2,068,971** | **1,393,520** | **8,943,763**         | **2,259,608** | **2,034,410** |

Read it this way:

- About 75% of each test set's wrong-family bytes were RIGHT claims in two copy shapes. Scorer 7 now credits
  them to their origin.
- About 12% are nested copies whose claims look right but carry no name signal.
- About 7.5% are declared-seed overreach. This is a matcher error, and the cap fixes it.
  - On test-esbuild the units the cap frees are partly re-claimed by other rules, so it nets −225 KB there.
- The rest (5-7%) is unmarked copies, sibling shippers and one truth artifact.
- **One shipper (the formatter package, two versions) is the truth label of 88-90% of all wrong-family bytes in
  each set, through two apps.** The top 3 apps carry 91-94%.

The 145 KB in test-esbuild's no-marker class is one truth artifact, labelled `src`. A package's own source map
lists `../src/x.ts`, and that path resolves inside `node_modules` to a directory that no package has. A scorer
rule for it was tried and refuted; see section 4.

**The candidate mechanisms from the brief, checked:**

| candidate                                                    | verdict                                                                                                                                                                                         |
| ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| unmarked copies crediting the shipper                        | **YES, the bulk of it.** The copies were not unmarked: they are rolldown-marked, and the origin layer's marker reader stopped too early (lazy bodies) or one level too shallow (nested chunks). |
| monorepo workspace packages outside node_modules / symlinked | No measurable share. The test installs are npm installs of the root manifest's dependencies; nothing is symlinked. The `src` artifact (above) is the one path-resolution class.                 |
| packages built from TS sources in the monorepo               | Same `src` class, about 0.1 MB.                                                                                                                                                                 |
| forks / generated code / families                            | Below 1% each in the residual.                                                                                                                                                                  |
| propagation                                                  | `down` + `module` claims are 15-18% of the scorer-7 residual (0.6 MB per set). They mostly ride on the nested-copy seeds above.                                                                 |
| version skew (resolved after the index's as-of date)         | No: the wrong bytes are whole-copy shapes, not nearest-version picks.                                                                                                                           |
| something else                                               | **Population and heavy tail.** GitHub-topic Electron apps list dev tooling (formatters, bundlers) as runtime dependencies. One such package pre-bundles megabytes of other packages' code.      |

## 2. Why dev never showed it: the population, measured

`scripts/scan_pkgs.py` reads the packages' OWN published tarballs; no scored set is read. It covers every
package version that a training app's bundle pulled 150 KB or more of. A package is counted when its files hold
50 KB or more of another package's code inside rolldown/tsdown module markers. The lazy-body shape is counted
separately.

| app population                                                | apps | bundle such a package | the lazy-body shape |
| ------------------------------------------------------------- | ---- | --------------------- | ------------------- |
| exp095 corpus (the dev / held-out / `apps` population)        | 338  | 15 (4.4%)             | 1 (0.3%)            |
| ... of which held-out 67                                      | 67   | 5                     | 0                   |
| exp108 training, CLI + Electron (the test split's population) | 1217 | 73 (6.0%)             | 15 (1.2%)           |
| ... Electron (GitHub topic search)                            | 894  | 67 (7.5%)             | (all 15)            |

- Only one package carries the lazy-body shape, in 11 of its versions.
- With 55 test apps, about 0.7 lazy-shape apps are expected. The split drew two, and each of them brings about
  4 MB of copies per bundle.

## 3. The dev analogue: corpus `pop`

`scripts/dev_pop.py` builds the corpus; its manifest is `experiments/096-library-matching-score/corpus-pop.json`.

- **Apps:** 46 of exp108's TRAINING apps, kind CLI or Electron.
  - None is held-out or test; this is checked against both selection files.
  - Each has a library closure of 25 MB or less.
- **Install:** exp108's own lock file (`npm ci`).
- **Builds:** exp100's Bun and esbuild recipes, unchanged. Bun built 45 apps and esbuild built 46, for
  **91 bundles**.

There are two strata:

- **mechanism (22 apps):** one app for each distinct set of pre-bundling packages that `scan_pkgs.py` found.
  A package is marked `+lazy` when it has the lazy shape. The choice within a set is seeded (112).
- **random (24 apps):** 12 CLIs and 12 Electron apps, seed 112.

Its truth layers were built exactly as for `apps`:

- the truth and the origin layer: `corpus.freeze_origins`;
- copies: exp104 `copies.py`, from reference label `pop-step4-structural` and index step4-v3;
- chained: exp106 `chained.py`.

Under scorer 6, `pop` reproduces test's precision gap and its mechanism mix:

| mechanism (pop-k2-rec, scorer 6) | pop-bun           | pop-esbuild       |
| -------------------------------- | ----------------- | ----------------- |
| nested copy (named + unnamed)    | 9,599,132 (68.3%) | 9,471,020 (62.8%) |
| lazy body after `//#endregion`   | 3,371,995 (24.0%) | 3,309,785 (21.9%) |
| no marker (whole print / other)  | 812,171 (5.8%)    | 1,938,348 (12.8%) |
| own region / other               | 275,799 (1.9%)    | 367,221 (2.4%)    |
| **total**                        | **14,059,097**    | **15,086,374**    |

Every fix below was designed and judged on `pop` and the other dev corpora. The test split was scored only to
report the summaries above.

## 4. The fixes

### Truth: scorer 7 (`origins.py` `RULES7`, frozen by `scorer7/origins7.py`)

Scorer 7 rebuilds the origin layer with two rules. With both rules off it is byte-identical to the frozen
scorer 4-6 layer on all five corpora: dev 69/69, apps 97/97, bundlers 27/27, test 191/191, pop 91/91
(`origins7.py verify`). `verify_s6.py` reproduces scorer 6's cards with the rules off (step4f-structural, 70/70).

- **`lazybody`.** In the shipper's file, a region names package O and ends with `//#endregion`. When the next
  line starts a lazy initialiser (`__esmMin(` / `__esm(`), the lines from there up to the next marker or the
  file's top-level `export` are O's.
- **`chunk`.** A built file is named after a package P when P is the longest `-`-prefix of the file's stem
  (`dist/jiti-Bb1dT6Cw.mjs` -> `jiti`, `_chunks/libs/cjs-module-lexer.mjs`). Such a file is a copy of P when
  both of these hold:

  - its owner declares P as a BUILD input: npm `devDependencies` or bundled dependencies, or its tarball's own
    third-party license collection (`dist/licenses/<P>/LICENSE`, a LICENSE.md `## <P>` section; `bundled.py`);
  - its owner does NOT declare P as a runtime dependency. A runtime dependency is installed next to the owner,
    so a file named after one is a wrapper.

  The owner is the shipper for a whole mapped file, or the region's package for a marker region (a nested
  copy). The version comes from the pnpm path in the marker when there is one.

  The registry manifests are cached under `/work/exp112-data/registry` (exp104 `fetch_metadata.py`), and the
  license collections under `/work/exp112-data/bundled`.

**Both-direction ledger** (`ledger.py`, scorer 6 -> 7, same claims):

- `pop`: wrong-family -> origin, 3,371,995 B by `lazybody` + 3,302,805 B by `chunk` (bun), and
  3,309,785 + 3,331,289 B (esbuild). Nothing else moved.
- held-out dev (k2 rec label): 0 B moved.
- apps-esbuild: 0 B moved.
- right -> wrong: 0 everywhere.

**Hand check:** 20 byte-weighted draws over the moved `pop` units, read in the bundle. They came from 6
distinct units:

- the lazy bodies are prettier plugin code (`labeled_statement`, the flow/markdown/acorn plugin tables);
- the chunk units are jiti's webpack-built `dist/jiti.cjs` (its `mlly` module ids, `createJiti`).

20/20 right.

**Refuted and reverted:** a `chainedpkg` rule. It was exp106's `chained` rule applied to a source that the
truth labels with a package that is not installed, such as the `src` artifact.

- On dev esbuild-app it flipped 333 KB from right to wrong. A package's own map that lists
  `../node_modules/.pnpm/ajv@8.18.0/node_modules/ajv/dist/core.js` points into the PUBLISHER's pnpm store.
- The path-derived label (`ajv`) is the origin, and the rule replaced it with the shipper. A missing path is
  no evidence that a label is bogus.
- The `src` class stays in the residual: about 0.15 MB on test-esbuild.

### Matcher: `library.discovery.declared_max_bytes` (default 0 = off; recommended 65536)

A declared seed reads a package's identity from text: an inlined `package.json`, or a scoped error prefix.
With the cap, a declared seed whose statement is bigger than N bytes is no seed. A manifest literal inside a
2 MB statement (a nested bundle's one factory) says the package is somewhere in it. It does not say the
statement is that package's.

Dev evidence for choosing the cap:

- Every declared seed judged right on dev, apps and pop is 8.7 KB or smaller.
- Every one over 64 KB (6.0 MB on `pop`) is wrong.
- 65536 sits inside a gap from 8.7 KB to 131 KB.

Results:

- pop: wrong 7.38 -> 4.37 MB (bun), 8.45 -> 5.88 MB (esbuild); recall +0.15 / +0.30.
- dev: 70/70 cards identical.
- apps-esbuild: identical.

Tests come first:

- `discovery_test.rs::a_declared_seed_never_claims_a_statement_bigger_than_the_declared_cap`;
- `library_config_test.rs::the_exp112_declared_cap_defaults_off`.

## 5. What is left on test (scorer 7 + cap, about 1.4 / 2.0 MB per set)

- Nested copies with no name signal: a pre-bundled plugin's `v3` / `v4` chunks hold tailwindcss internals and
  jiti, and prettier's own `index.mjs` holds editorconfig.
  - The claims are mostly right by content.
  - No marker, manifest or license names the inner package, so the truth cannot credit them yet.
  - A copies-style rule (index code + declaration + named) needs a declaration that these owners do not
    publish.
- Sibling shippers: the claim names another package that bundles the same origin (parse5 inside vite named
  `@stencil/core`, memfs named `@napi-rs/wasm-runtime`). These are genuinely wrong.
  - The index has the copier's rows, not the origin's.
  - exp110's dedup is the lever, and it needs an origin-crediting truth, which scorer 7 now gives for the
    marked shapes.
- The `src` truth artifact (section 4).

## Re-score (scorer 6 -> 7, every committed label once)

All 80 committed labels and this lane's 11 were re-scored (`/work/exp112-data/rescore-list.txt`). Every run
exited 0. `experiments/107-zero-app-claims/rescore_diff.py` shows what moved:

- **dev (55 labels), apps (6), bundlers (2):** only `schema`, `config.origins7Sha256` and `copyBytes`.
  `copyBytes` is the bytes the exp104 copies layer credited: those units now get their origin from `chunk`.
  No precision, recall or claim verdict moved.
- **pop:** wrong-family -> origin only (section 4).
- **test (23 labels, summaries only):** precision, recall, F0.5, F1, named / origin / wrong-family bytes.
  App claimed stays 0 everywhere.

Selected test labels, precision and F0.5, scorer 6 -> 7:

| label                            | test-bun                      | test-esbuild                  |
| -------------------------------- | ----------------------------- | ----------------------------- |
| test-exp101-c2-structural        | P 75.4->96.6, F0.5 66.1->84.6 | P 77.4->97.0, F0.5 68.3->85.6 |
| test-exp108-k2-structural        | P 79.5->94.2, F0.5 73.3->86.9 | P 83.0->95.2, F0.5 79.0->90.6 |
| test-exp108-k2-guards-structural | P 80.1->95.0, F0.5 73.7->87.3 | P 83.7->96.0, F0.5 79.5->91.1 |
| test-exp101-call2k-structural    | P 79.2->94.7, F0.5 71.8->85.8 | P 80.5->94.8, F0.5 73.3->86.4 |
| test-step4f-structural           | P 52.7->88.1, F0.5 34.0->56.8 | P 51.4->69.0, F0.5 33.8->45.4 |

The rule fires on webpack and rollup too. For example, test-exp107-main-structural webpack goes from P 85.8 to
97.9 and rollup from 75.9 to 93.2.

**After merging main `b4db2b76` (exp109 defaults, exp114 keys):** its 35 new labels (exp109-\*, exp114-\*, and their test summaries) were re-scored once under scorer 7, all exit 0. The moves are the same as above: dev, apps and bundlers move only `schema`, `origins7Sha256` and `copyBytes`; test moves only the truth-derived summary fields. App claimed is 0 everywhere. For example, test-exp109-final-k2-structural (the shipped default config) goes from bun P 80.6 to 95.4 / F0.5 74.1 to 87.7, and from esbuild P 83.6 to 95.9 / F0.5 79.4 to 91.1. test-exp114-allf-k2-structural goes from bun F0.5 74.3 to 88.0 and esbuild 79.5 to 91.2. `declared_max_bytes` stays default-off beside exp109's `recommended()`, which is unchanged.

**Merge note:** exp113 also works off main. Whichever lane lands second re-scores once under the combined
scorer.

## Neutrality

The crates change is the default-off cap. The warm run, 2.1.215 -> 2.1.216, `1a12c1db` vs main `829da98f`,
cache +0 / +0, came back **NEUTRAL: 0 differing files, 0 differing lines**, identical exit code 0.

## Reproduce

```bash
python3 -I experiments/112-test-precision/scripts/scan_pkgs.py /work/exp108-data/apps <out.jsonl>   # tarball scan
python3 -I experiments/112-test-precision/scripts/dev_pop.py plan && python3 -I … build --jobs 6
<venv> -I experiments/112-test-precision/scripts/dev_pop.py freeze --jobs 12
npm run eval -- library pop-step4-structural --index /work/exp103-data/ix/step4-v3 --rule structural --corpus pop
<venv> -I experiments/104-truth-audit/copies.py pop-step4-structural /work/exp112-data/registry \
  crates/humanify-core/src/libindex/families-derived.json --jobs 1 --corpus pop
<venv> -I experiments/106-matcher-hygiene/scorer5/chained.py build --corpus pop
# scorer 7's layer, per corpus (dev apps bundlers pop test):
<venv> -I experiments/112-test-precision/scorer7/origins7.py needs|owners --corpus C <out.json>
python3 -I experiments/104-truth-audit/fetch_metadata.py /work/exp112-data/registry <needs.json>
python3 -I experiments/112-test-precision/scorer7/bundled.py <owners.json>
<venv> -I experiments/112-test-precision/scorer7/origins7.py verify|build --corpus C
# the mechanism table (aggregates only; rows stay under /work):
<venv> -I experiments/112-test-precision/rows.py <corpus> <label> rows.jsonl [--s6]
python3 -I experiments/112-test-precision/evidence.py <data dir> <label> <index> rows.jsonl ev.jsonl
<venv> -I experiments/112-test-precision/mech.py <corpus> ev.jsonl --json mechanisms.json
python3 -I experiments/112-test-precision/ledger.py rows-s6.jsonl rows-s7.jsonl
<venv> -I experiments/112-test-precision/verify_s6.py <label> [--corpus C] [--show]
```
