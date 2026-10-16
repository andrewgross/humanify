# exp120: truth families over every corpus label, and chained sources naming no npm package (scorer 9)

## STATUS (2026-10-15)

- **DONE on branch `exp120-truth-families`** (off main `8ba487df`). NOT merged.
- **Two scorer-side gaps from exp119, both fixed generically:**
  1. **Truth packages outside the families universe.** The universe exp104's `derive_families.py` reads is now the
     exp119 universe (k2-all + earlier names, 17,316) plus every package NAME the truth of any corpus labels with:
     the truths' source tables, the chained layers (scorer 5 and 9), the scorer-7 origin layers and the copies
     layers, for dev, apps, bundlers, pop, test and test2 (`universe.py`). 718 new names, 18,034 in all. For test
     and test2 only the names were read; no per-app result was opened (the script prints counts only).
     Re-derived with exp104's rules unchanged. Links 5,562 -> **5,751** (repo 5,543, rename 122, fork 86); every
     pair the old file joined is still joined (union-find check), so it is a strict superset. `graphlib ~
@snyk/graphlib` is now a fork link (`github.com/dagrejs/graphlib -> github.com/snyk/graphlib`).
     Red/green: `families_test.rs`, `the_shipped_derived_links_cover_every_corpus_truth_label`, failed on the old
     file and passes on the new one.
  2. **Chained source paths naming a package npm does not have.** New truth rule `npmchain`
     (`scorer9/chained9.py`, frozen per corpus as `chained9[-<corpus>].json` + `<id>.chained9.json.gz`, applied in
     `truth_audit.patch_sources` after scorer 5's `chained`). primevue's own maps list `../../src/<x>.js`, which
     resolves to `node_modules/@primevue/src/<x>.js`; the truth's path rule read that as the package
     `@primevue/src`, which is not on npm and so sits in no family. The rule keeps the real package whose map
     the bundler followed.
- **Scorer 9** (`exp096-score/9`). With both changes undone (RULES9 off, scorer 8's links file) it reproduces the
  committed scorer-8 output exactly: `verify_s8.py` on `exp119-head`, `pop-exp119-k2all-st` and
  `exp119-k2all-st-apps`, every card and summary IDENTICAL. **All 237 committed labels were re-scored once**
  (dev 81, apps 41, bundlers 18, pop 16, test 56, test2 25; the six `*exp120*` labels were scored at run time). After main `d2d08a05` (exp118) was merged in, its six `*exp118-dl25k*` labels were re-scored once
  under scorer 9 with `rescore_many.py` (test and test2: summaries only). No set's wrong-family bytes rose; they
  fell on apps (esbuild -50,002, rollup -213,386, webpack -368,247), pop (bun -24,558, esbuild -99,124) and
  test2-esbuild (-5,959). App claimed is 0. The ledger was not re-run for these six labels.
- **Ledger, both directions, every label: right -> wrong = 0 bytes everywhere.** No GOOD -> WRONG, GOOD ->
  UNJUDGED or WRONG -> UNJUDGED transition on any label of any corpus. Every move is WRONG -> GOOD or GOOD ->
  GOOD (`family` -> `right` when a chained source now carries the claimed package itself).
- **Hand check:** 20 byte-weighted relabelled DEV units (seed 120). **20/20 correct as family verdicts**, one of
  them debatable at the package level (a claim naming a sibling addon of the same repository).
- **Matcher (shipped defaults, k2-all): identical claims on every corpus.** The new links join no pair the
  matcher's index-restricted family table can act on. The reports' claims are identical on all 285 dev-corpus
  reports (dev 70, apps 97, bundlers 27, pop 91). P / R / F0.5 are equal on every set, test and test2 included.
  App claimed is 0 everywhere.
- **Gate:** ALL 13 STAGES PASSED, bare, at `2761fe53` and again on the final tree (see the commit log).
- **Neutrality, warm, vs main `8ba487df`, pair 2.1.215 -> 2.1.216** (`--priors /work/exp050-cold --cache
/work/neutrality-cache`, under `flock /work/neutrality.lock`, from a clean detached worktree of `2761fe53`):
  default mode **NEUTRAL, 0 files / 0 lines, cache +0 / +0, exit 0 / 0**. Report mode (`--pipeline-set
library.matching=report --pipeline-set library.index=/work/exp113-data/ix/k2-all`): **NEUTRAL, 0 / 0, +0 / +0,
  exit 0 / 0**, and `.humanify/library-report.json` is byte-identical too.

## Rule `npmchain` (scorer 9)

A source the truth labels with a package P is relabelled when all of these hold:

1. its path, resolved against the bundle's directory, does not exist on disk;
2. P is not a package on npm. The rule reads `P@latest` from the exp104 registry cache and fetches it once when
   it is missing. A name npm cannot hold at all (a leading `.`, for example `@js-sdsl/.build-data`) counts as not
   on npm without a fetch;
3. some installed package's own map lists that path. This is scorer 5's search, factored out of
   `scorer5/chained.py` as `map_holders` and `holder_row` and shared, not copied. The refactor reproduces every
   frozen scorer-5 layer exactly on dev and bundlers. The first holder H, by path order, must be on npm and must
   not be P.

The source then takes H, with H's installed version. Each layer row keeps the old label, and the row applies only
to a source that still carries that label. Every npm verdict the rule read is recorded in the manifest. For test
and test2 the manifest records only their count and sha, because the names come from the private corpora.

Relabelled sources (dev corpora by name):

| corpus                | sources  | old label -> holder (sources)                                                                                                                                                                                                                                                 |
| --------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| dev (esbuild-app)     | 96       | `@modelcontextprotocol/core-internal` -> `@modelcontextprotocol/client` 87 / `server` 3; `@js-sdsl/.build-data` -> `@js-sdsl/ordered-map` 6                                                                                                                                   |
| apps (esbuild)        | 204      | `@sentry/src` -> `@sentry/core` 57 / `utils` 46 / `types` 8; `0-framework` -> `@prisma/composer` 22; `@stoplight/src` -> `@stoplight/yaml` 21 and others; `@babel/babel-helper-*` -> `@babel/parser` 21; `@jridgewell/src` -> `@jridgewell/trace-mapping` 8                   |
| pop (esbuild)         | 497      | `@modelcontextprotocol/core-internal` -> `client` 264 / `server` 56; `@primevue/src` -> `@primevue/icons` 94 / `@primevue/core` 20; `@babel/babel-helper-*` -> `@babel/parser` 36; `@rrweb/*` -> `@rrweb/record`; `@octokit/src`; `@sentry/replay-worker` -> `@sentry/replay` |
| bundlers              | 0        |                                                                                                                                                                                                                                                                               |
| test, test2 (esbuild) | 169, 574 | counts only                                                                                                                                                                                                                                                                   |

**Not relabelled, on purpose (`realNameNotInstalled`: pop 2,407, apps 945, dev 324, test 3,223, test2 1,590
sources).** These are chained paths whose package name does exist on npm but is not installed there. There are
two kinds, and only the registry check tells them apart. Most are real vendored copies that the truth already
names correctly: `ajv`, `ajv-formats` and `fast-uri` inside `@modelcontextprotocol/client`'s map, `css-tree`
inside `@bramus/specificity`, `@ai-sdk/provider-utils` inside `@openrouter/ai-sdk-provider`. Relabelling those to
the holder would make right claims wrong. A minority are directory names that happen to be squatted npm names:
`src` (`node_modules/src/...` from primevue, drizzle-orm, ng-packagr and async-validator maps) and `shared`
(lucide). They have the same defect as `@primevue/src`, but the npm test cannot see it. A rule for them needs
other evidence, for example that the named package's published files do not contain the path. Follow-up, not
done here.

## Truth ledger (scorer 8 -> 9; `ledger9.py`; bytes)

`ledger9.py` labels every claimed unit exactly as `score.py` does, twice. The first pass uses RULES9 off with
scorer 8's links file, and the second uses scorer 9. A changed verdict is charged to `npmchain` when the unit's
label or origin changed. Otherwise it is charged to the new link kinds that alone join the pair (exp119's
`Kinds`). Per-label JSON: `/work/exp120-data/ledger/<corpus>.json`; dev-corpora rows in `<corpus>.jsonl`.

| corpus   | labels (with any move) | right -> wrong | wrong -> right, all labels             | other                                                             |
| -------- | ---------------------- | -------------- | -------------------------------------- | ----------------------------------------------------------------- |
| dev      | 81 (40)                | 0              | 0                                      | GOOD -> GOOD 106,757 (npmchain, esbuild-app: `family` -> `right`) |
| apps     | 41 (27)                | 0              | 5,528,070                              | -                                                                 |
| bundlers | 18 (0)                 | 0              | 0                                      | -                                                                 |
| pop      | 16 (16)                | 0              | 1,495,478                              | GOOD -> GOOD 3,036,230                                            |
| test     | 56 (0)                 | 0              | 0                                      | -                                                                 |
| test2    | 25 (all aggregate)     | 0              | 93,478 (npmchain 89,440, rename 4,038) | GOOD -> GOOD 5,096,587 (npmchain)                                 |

On the shipped-default labels (`exp119-head*` / `*exp119-k2all-st*`), wrong -> right bytes by kind:

| set                      | wrong -> right | by kind                                                                      |
| ------------------------ | -------------- | ---------------------------------------------------------------------------- |
| apps-esbuild             | 50,770         | npmchain 50,770                                                              |
| apps-rollup              | 267,643        | repo 214,002, fork 53,641                                                    |
| apps-webpack             | 428,305        | repo 368,247, fork 54,495, chain 5,563 (`@snyk/graphlib`)                    |
| pop-bun                  | 42,166         | fork 24,497, repo 17,669                                                     |
| pop-esbuild              | 116,764        | npmchain 70,436 (`@primevue/src`: exp119's +68 KB), fork 24,938, repo 21,390 |
| test2-esbuild            | 5,959          | npmchain                                                                     |
| test2-webpack            | 673            | rename                                                                       |
| dev sets, bundlers, test | 0              | -                                                                            |

The ledger matches the cards. Every wrong-family drop in the next table equals the ledger's wrong -> right bytes.

Scorer 8 -> 9 on the shipped-default labels (P / R / F0.5 in %; the matcher did not run again):

| set                                | P              | R              | F0.5           | wrong-family bytes     |
| ---------------------------------- | -------------- | -------------- | -------------- | ---------------------- |
| cc, heldout, esbuild-app, wrangler | =              | =              | =              | =                      |
| apps-esbuild                       | 98.73 -> 98.95 | 89.71 -> 89.91 | 96.79 -> 97.00 | 295,428 -> 244,658     |
| apps-rollup                        | 98.57 -> 98.92 | 77.12 -> 77.40 | 93.38 -> 93.71 | 1,095,684 -> 828,041   |
| apps-webpack                       | 98.61 -> 99.39 | 72.75 -> 73.33 | 92.06 -> 92.79 | 764,663 -> 336,358     |
| dev-rollup, dev-webpack            | =              | =              | =              | =                      |
| pop-bun                            | 95.08 -> 95.13 | 77.13 -> 77.17 | 90.85 -> 90.90 | 4,040,238 -> 3,998,072 |
| pop-esbuild                        | 93.52 -> 93.65 | 79.54 -> 79.66 | 90.34 -> 90.47 | 5,600,477 -> 5,483,713 |
| test-bun/esbuild/rollup/webpack    | =              | =              | =              | =                      |
| test2-bun, test2-rollup            | =              | =              | =              | =                      |
| test2-esbuild                      | 99.41 -> 99.43 | 74.21 -> 74.23 | 93.09 -> 93.10 | 190,602 -> 184,643     |
| test2-webpack                      | 99.14 =        | 69.22 =        | 91.25 =        | 1,029,629 -> 1,028,956 |

The recall denominator does not move. `npmchain` relabels a library source to another library package, and a new
link changes only `family()`. Recall rises because claims newly judged right or family count as named.

`rescore_diff.py` (exp107), comparing main's committed results with these: on dev, bundlers and test, only `schema`,
`config.familiesSha256` and `config.chained9Sha256` moved, plus, on dev, claim-verdict fields of 40 cards
(`claimedBytes.family` -> `.right`, the esbuild-app npmchain move) and the miss classes that follow them. On apps
(69 cards), pop (121 cards) and test2 (22 of 25 summaries), the claim-verdict fields moved (`claimedBytes`,
`totals`, `indexSeeds`) with the miss shares that follow `family()`. No report, index or claim field moved
anywhere.

## Hand check (`handcheck.json`; exp119's `handcheck_draw.py --seed 120`, byte-weighted, dev corpora only)

The pool is 1,738 distinct relabelled units from the dev, apps, bundlers and pop ledger rows; test and test2 were
not drawn. Each unit's text was read from the frozen bundle.

| draws | claim <- truth label (old label)                                                   | verdict                                                                                                                                             |
| ----- | ---------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| 4     | `@modelcontextprotocol/server` <- `server` (`@modelcontextprotocol/core-internal`) | correct: MCP error codes, request and result schemas, protocol-revision table. Only server's tarball ships it; the claim moves from family to right |
| 5     | `yaml-ast-parser` <- `@stoplight/yaml-ast-parser`                                  | correct: GitHub fork (YAML tag and type code)                                                                                                       |
| 3     | `sonarqube-scanner` <- `@sonar/scan`                                               | correct: one repository (SonarSource/sonar-scanner-npm)                                                                                             |
| 2     | `@xterm/addon-ligatures` <- `xterm-addon-ligatures`                                | correct: the addon renamed into `@xterm`, one repository                                                                                            |
| 1     | `@xterm/addon-image` <- `xterm-addon-image`                                        | correct: same rename                                                                                                                                |
| 2     | `primevue` <- `@primevue/icons` (`@primevue/src`)                                  | correct: PrimeVue icon render functions, primevue monorepo                                                                                          |
| 1     | `@primevue/core` <- `@primevue/core` (`@primevue/src`)                             | correct: FilterService; the claim moves from family to right                                                                                        |
| 1     | `@dagrejs/graphlib` <- `@snyk/graphlib`                                            | correct: graphlib's PriorityQueue; Snyk's GitHub fork                                                                                               |
| 1     | `xterm-addon-canvas` <- `xterm-addon-webgl`                                        | correct as family, debatable as package: the unit is the WebGL addon; the claim names its sibling in the same repository                            |

## Matcher: main's shipped defaults on k2-all, before -> after the new links file

"Before" is exp119's labels run with main `8ba487df`'s defaults: `exp119-head` (dev), `exp119-head-bundlers`,
and `exp119-k2all-st` for apps, pop, test and test2. "After" is `exp120-k2all-def*`, this branch's binary with no
`--pipeline-set`. Both sides are scored under scorer 9, so the table isolates the matcher. Values are
P / R / F0.5 in %. test and test2 are summaries only.

| set           | before P / R / F0.5    | after P / R / F0.5     | dF0.5 | wrong-family bytes     | app    |
| ------------- | ---------------------- | ---------------------- | ----- | ---------------------- | ------ |
| cc            | 100.00 / 81.90 / 95.77 | 100.00 / 81.90 / 95.77 | +0.00 | 0 -> 0                 | 0 -> 0 |
| esbuild-app   | 96.25 / 65.50 / 87.99  | 96.25 / 65.50 / 87.99  | +0.00 | 125,529 -> 125,529     | 0 -> 0 |
| heldout       | 99.24 / 66.69 / 90.42  | 99.24 / 66.69 / 90.42  | +0.00 | 154,751 -> 154,751     | 0 -> 0 |
| wrangler      | 95.99 / 18.53 / 52.28  | 95.99 / 18.53 / 52.28  | +0.00 | 5,652 -> 5,652         | 0 -> 0 |
| apps-esbuild  | 98.95 / 89.91 / 97.00  | 98.95 / 89.91 / 97.00  | +0.00 | 244,658 -> 244,658     | 0 -> 0 |
| apps-rollup   | 98.92 / 77.40 / 93.71  | 98.92 / 77.40 / 93.71  | +0.00 | 828,041 -> 828,041     | 0 -> 0 |
| apps-webpack  | 99.39 / 73.33 / 92.79  | 99.39 / 73.33 / 92.79  | +0.00 | 336,358 -> 336,358     | 0 -> 0 |
| dev-rollup    | 99.87 / 77.53 / 94.43  | 99.87 / 77.53 / 94.43  | +0.00 | 2,114 -> 2,114         | 0 -> 0 |
| dev-webpack   | 99.64 / 62.89 / 89.22  | 99.64 / 62.89 / 89.22  | +0.00 | 4,139 -> 4,139         | 0 -> 0 |
| pop-bun       | 95.13 / 77.17 / 90.90  | 95.13 / 77.17 / 90.90  | +0.00 | 3,998,072 -> 3,998,072 | 0 -> 0 |
| pop-esbuild   | 93.65 / 79.66 / 90.47  | 93.65 / 79.66 / 90.47  | +0.00 | 5,483,713 -> 5,483,713 | 0 -> 0 |
| test-bun      | 97.23 / 66.39 / 88.96  | 97.23 / 66.39 / 88.96  | +0.00 | 1,235,745 -> 1,235,745 | 0 -> 0 |
| test-esbuild  | 96.42 / 75.96 / 91.49  | 96.42 / 75.96 / 91.49  | +0.00 | 1,946,978 -> 1,946,978 | 0 -> 0 |
| test-rollup   | 97.89 / 68.39 / 90.12  | 97.89 / 68.39 / 90.12  | +0.00 | 1,607,380 -> 1,607,380 | 0 -> 0 |
| test-webpack  | 99.19 / 70.51 / 91.73  | 99.19 / 70.51 / 91.73  | +0.00 | 865,443 -> 865,443     | 0 -> 0 |
| test2-bun     | 98.71 / 77.70 / 93.64  | 98.71 / 77.70 / 93.64  | +0.00 | 423,750 -> 423,750     | 0 -> 0 |
| test2-esbuild | 99.43 / 74.23 / 93.10  | 99.43 / 74.23 / 93.10  | +0.00 | 184,643 -> 184,643     | 0 -> 0 |
| test2-rollup  | 96.18 / 74.74 / 90.96  | 96.18 / 74.74 / 90.96  | +0.00 | 1,756,319 -> 1,756,319 | 0 -> 0 |
| test2-webpack | 99.14 / 69.22 / 91.25  | 99.14 / 69.22 / 91.25  | +0.00 | 1,028,956 -> 1,028,956 | 0 -> 0 |

The claims are identical, not just the totals. Every report's `claims` list is equal before and after on dev,
apps, bundlers and pop (285 reports). test and test2 were compared as summaries only.

## Tools

- `universe.py`: every truth package name of every corpus, merged into the base universe. It prints counts only.
- `scorer9/chained9.py`: builds the frozen `npmchain` layer (`build --corpus C`). It prints per-set aggregates only.
- `ledger9.py`: the scorer 8 -> 9 ledger, both directions, charged to `npmchain` or a link kind.
- `verify_s8.py`: proves that scorer 9 with RULES9 off and scorer 8's links reproduces committed scorer-8 output.
- `rescore_many.py`: re-scores many labels of one corpus in one process. Each entry's truth is prepared once
  (load, source patches, origin layer, re-decode: about 85% of a re-score) and served from a per-run pickle cache
  to later labels. Its output was checked identical to plain `score.py` on 2 apps labels (82 files). It made the full re-score a few hours of work instead of an overnight job.
- `handcheck.json`, from `experiments/119-families-full/handcheck_draw.py`. This lane gave that script `--seed` and
  made it carry `oldLabel`.
- Labels: `exp120-k2all-def{,-apps,-bundlers}` and `{pop,test,test2}-exp120-k2all-def`; all 237 older labels were
  re-scored.

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
# 1. the npmchain layers (fetches missing npm verdicts into the exp104 cache)
for c in dev apps bundlers pop test test2; do $PY -I experiments/120-truth-families/scorer9/chained9.py build --corpus $c; done
# 2. universe + metadata + GitHub fork data + derive (exp104 rules)
python3 -I experiments/120-truth-families/universe.py /work/exp119-data/universe-k2all.json /work/exp120-data/universe-s9.json
python3 -I experiments/104-truth-audit/fetch_metadata.py /work/exp104-data/registry /work/exp120-data/universe-s9.json
python3 -I experiments/104-truth-audit/derive_families.py /work/exp104-data/registry /work/exp120-data/universe-s9.json /work/exp120-data/fam-pre-gh.json
python3 -I experiments/119-families-full/fetch_github.py /work/exp104-data/registry /work/exp120-data/fam-pre-gh.json
python3 -I experiments/104-truth-audit/derive_families.py /work/exp104-data/registry /work/exp120-data/universe-s9.json \
  crates/humanify-core/src/libindex/families-derived.json && npx prettier --write crates/humanify-core/src/libindex/families-derived.json
# 3. checks, ledger, re-score
$PY -I experiments/120-truth-families/verify_s8.py <scorer-8 families-derived.json> exp119-head
$PY -I experiments/120-truth-families/ledger9.py <scorer-8 families-derived.json> <labels...> --corpus C --json C.json [--rows C.jsonl]
$PY -I experiments/120-truth-families/rescore_many.py --corpus C --cache <scratch dir> <labels...>
# 4. matcher
C="--index /work/exp113-data/ix/k2-all --rule structural --cpus 0-11 --jobs 4"
npm run eval -- library exp120-k2all-def $C     # + --corpus apps|bundlers|pop|test|test2
# 5. neutrality (clean detached worktree of the commit)
flock /work/neutrality.lock npm run eval -- neutrality 8ba487dfb 2.1.215:2.1.216 --priors /work/exp050-cold --cache /work/neutrality-cache
flock /work/neutrality.lock npm run eval -- neutrality 8ba487dfb 2.1.215:2.1.216 --priors /work/exp050-cold --cache /work/neutrality-cache \
  --pipeline-set library.matching=report --pipeline-set library.index=/work/exp113-data/ix/k2-all
```
