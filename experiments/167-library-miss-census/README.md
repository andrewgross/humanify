# exp167 — what library code is NOT matched to its package, and why (census only)

STATUS: CENSUS COMPLETE (2026-10-31). Branch `exp167-library-miss-census` off main `c7cafbc74`. **Nothing under
`crates/` or the TS harness changed** (two new Python scripts and data only), so no gate run and no neutrality run.
Every number is main's binary (crates byte-identical to `6e5f16c93`, the exp165 reports' binary — verified below)
in names mode on index `cb64c1f5` (format /4, edges/2, names/1; 65,839 versions / 9,896 packages). Fixes are
recommended, not built.

## Totals first

**bun-dev** (89 + 1 = all 90 bundles, 45 apps × Bun `--target node|bun`, full source-map truth, scorer 14):

| quantity                                                                       |           bytes |                                                  share |
| ------------------------------------------------------------------------------ | --------------: | -----------------------------------------------------: |
| library bytes (truth)                                                          | **214,305,418** |                                                   100% |
| named right by discovery's claims (the scorer's recall)                        |     176,073,145 | **82.16%** (node 82.70 / bun 81.59, P 99.742 / 99.825) |
| missed by the claims                                                           |  **38,232,273** |                                                 17.84% |
| …of which names mode still PLACES under the right `vendor/<pkg>/` (module key) |      13,806,258 |                                                  6.44% |
| **missed after placement (claims + whole-module placement)**                   |  **24,426,015** |              **11.40%** (placement-view recall 88.60%) |
| claimed as the WRONG package (precision; inside the missed bytes)              |         384,084 |                                                  0.18% |

**cc 2.1.216** (620-unit labelled sample, stratified estimate over src/'s 15.28 MB of statement units): library
2.22 MB, named 1.82 MB = **81.90%** (P 100.0), missed **402 KB** (28 of 108 sampled library units). The 5.05 MB of
CommonJS factories are outside the sample (rule 8: not judged here).

Both headline recalls reproduce the stated baselines exactly: the official scorer on this label
(`data/scorer-summary-bundev.json`, label `exp167-names-cb64`) reads bun-dev-node P 99.742 / R 82.702 and
bun-dev-bun P 99.825 / R 81.593, app claimed 0; the census's own verdicts equal the scorer's named bytes
(176,073,145) and wrong-family bytes (384,084) to the byte.

### Missed bytes by cause (bun-dev, exclusive, ranked by bytes)

"Placed right" = the unit's factory is placed by names mode under the right package anyway (bundleOrder's
`indexPath`: a claim fold or the whole-module key), so it is a loss for the claims (and for identifier
restoration) but not for the vendor path. "Residual" = missed after placement.

| cause                                                                            | missed MB |   units |  % of lib | placed right MB | **residual MB** | top packages (bytes / apps bundling it)                                                                   |
| -------------------------------------------------------------------------------- | --------: | ------: | --------: | --------------: | --------------: | --------------------------------------------------------------------------------------------------------- |
| 4. guard: isolation veto                                                         |      7.05 |    5182 |      3.29 |            5.01 |        **2.04** | highlight.js 1.56M/1, lucide-react 1.50M/4, @ant-design/icons-svg 1.41M/1, playwright-core 389K/1         |
| 1. not indexed (never pinned)                                                    |      6.99 |    7016 |      3.26 |            0.01 |        **6.98** | kryptokrona-crypto 2.80M/1, @liquify/specs 1.58M/1, @altimateai/dbt-integration 397K/1                    |
| 1. not indexed (stop-listed helper package)                                      |      0.42 |     354 |      0.19 |            0.00 |            0.42 | @babel/helpers 211K/1, tslib 190K/8, @babel/runtime 16K/1                                                 |
| 1. not indexed (JS-less package: node-releases)                                  |      0.07 |       2 |      0.03 |            0.00 |            0.07 | node-releases 67K/1                                                                                       |
| 4. guard: factory whole-seed (`guard.presence`)                                  |      4.04 |     222 |      1.89 |            0.02 |        **4.02** | azure-devops-node-api 1.05M/6, libsodium 843K/1, @xterm/xterm 487K/1, MCP client+server 590K/1            |
| 2. sub-floor (every print < 20 nodes: no row can exist)                          |      3.61 |  102854 |      1.69 |            2.30 |            1.31 | lucide-react 704K/4, entities 662K/13, @ant-design/icons 285K/1, emoji-regex 119K/6                       |
| 2'. lookup floor (family holds the print; lookup reads ≥ 30 nodes / ≥ 3 content) |      3.55 |   12792 |      1.66 |            3.27 |            0.27 | undici 1.16M/8, entities 1.00M/13, @ant-design/icons 407K/1, victory-vendor 152K/2                        |
| 3. version absent, neighbour versions hold the file (skew)                       |      2.41 |     855 |      1.13 |            0.28 |        **2.14** | lucide-react 1.17M/4, i18n-iso-countries 240K/1, @octokit/plugin-rest-endpoint-methods 164K/1             |
| 3. version absent, no indexed version holds the file                             |      0.27 |     210 |      0.12 |            0.10 |            0.16 | @vscode/vsce 134K/4, @antfu/eslint-config 80K/1                                                           |
| 4. guard: multi-package (> 3 owners)                                             |      2.40 |    3527 |      1.12 |            1.28 |            1.12 | @ant-design/icons 1.18M/1, @microsoft/applicationinsights-web-basic 497K/2, @microsoft/1ds-core-js 423K/2 |
| 5. print differs, file indexed at that version                                   |      1.59 |     535 |      0.74 |            0.08 |            1.51 | lucide-react 963K/4, terser 299K/1, playwright-core 151K/1                                                |
| 5. print differs, file not indexed at that version                               |      0.92 |     359 |      0.43 |            0.10 |            0.82 | lucide-react 478K/4, jsdom 191K/2, localforage 64K/1, cli-spinners 58K/4                                  |
| 4/5. held by another package only (shared code, carriers, map defects)           |      1.34 |     243 |      0.63 |            0.06 |            1.28 | playwright-core 813K/1, moment 151K/3, encoding-sniffer 112K/5                                            |
| 4. guard: multi-family tie fenced                                                |      1.07 |     903 |      0.50 |            0.43 |            0.64 | tr46 290K/3, prompts 207K/2, encoding-sniffer 108K/5, css-tree 104K/3                                     |
| 7. guard: other (ambiguous after relaxing every guard)                           |      0.84 |    1932 |      0.39 |            0.31 |            0.53 | playwright-core 184K/1, @azure/core-rest-pipeline 74K/4, terser 71K/1                                     |
| 4. guard: tie lost (another family scores higher)                                |      0.69 |    1641 |      0.32 |            0.21 |            0.48 | victory-vendor 99K/2 (→@antv/vendor), color-name 89K/12 (→color-convert), react-dom 70K/7                 |
| 4. guard: position window (< 100 nodes, no same-package claim within 10)         |      0.47 |    3038 |      0.22 |            0.34 |            0.13 | @ant-design/icons 208K/1, prompts 21K/2, debug 21K/23                                                     |
| 6. claimed as the WRONG package (precision)                                      |      0.38 |    2457 |      0.18 |  0 (0.25 wrong) |            0.38 | hyperdht-hugin 141K (→hyperdht), string_decoder 40K (→readable-stream), @vscode/prompt-tsx 36K            |
| 4. guard: specificity (index module > 4× + 4 KB)                                 |      0.12 |      66 |      0.05 |            0.00 |            0.12 | terser 61K/1, sql.js 44K/1                                                                                |
| **total**                                                                        | **38.23** | 153,569 | **17.84** |       **13.81** |       **24.43** |                                                                                                           |

Grouped into the brief's seven causes (residual after placement in brackets): **1 not indexed 7.47 MB [7.47]**;
**2 under the floors 7.16 MB [1.58]**; **3 version skew 2.68 MB [2.30]**; **4 a guard refused 18.20 MB [10.58]**
(veto 7.05, whole-seed 4.04, multi-package 2.40, held-by-other 1.34, tie-fenced 1.07, tie 0.69, window 0.47,
specificity 0.12); **5 the print differs 2.51 MB [2.33]**; **6 wrong package 0.38 MB** (precision, apart);
**7 other 0.84 MB [0.53]**.

### cc 2.1.216 sample (stratified estimate; units are sample units)

| cause                                                        | missed KB | sample units | packages                                                                                               |
| ------------------------------------------------------------ | --------: | -----------: | ------------------------------------------------------------------------------------------------------ |
| not indexed                                                  |       105 |            7 | @anthropic-ai/mcpb 60K, @anthropic-ai/bedrock-sdk 30K, @bufbuild/cel 12K, foundry/google-cloud SDKs 3K |
| print differs (version indexed, no file in the sample truth) |       103 |            3 | @aws-sdk/client-bedrock 67K, client-sso 20K, client-sts 16K                                            |
| version absent                                               |        62 |            4 | @azure/identity 35K, @anthropic-ai/sandbox-runtime 27K                                                 |
| held by another package                                      |        46 |            1 | @smithy/smithy-client (held by drizzle-kit: a carrier)                                                 |
| lookup floor                                                 |        35 |            2 | lodash-es                                                                                              |
| sub-floor                                                    |        30 |           10 | @smithy/types 16K, lodash-es 4K, AWS clients 6K                                                        |
| isolation veto                                               |        21 |            1 | p-map                                                                                                  |
| **total**                                                    |   **402** |       **28** | (0 wrong-package, 0 of 511 sampled app units claimed)                                                  |

28 units cannot rank causes with any confidence (rule 11: one unit is 7–60 KB of estimate); read it as "the same
causes appear", led by not-indexed (Anthropic's own unpublished/new SDK packages) and AWS clients whose prints
differ.

## Method

- **Reproduction.** exp165's 89 bun-dev reports (`humanify discover --unit-prints --call-context --bundle-order`,
  config `discover.toml` = `library.matching = "names"`, `names_unlocked`, `lookup.agreement = false`, the cb64c1f5
  index) were reused after proving them main's: main's binary rebuilt here (`cargo build --release --locked`,
  sha256 `a8cbd25f…`) re-ran three of them byte-identically (`cli_doctoc` node, `helm-intellisense` node, and
  cc 2.1.216, whose `libraryDiscovery` is identical — only `bundlerOverride` differs because this run passed
  `--bundler bun`). The 90th bundle, **`bun-dev-bun-vscode_gh-tim-koehler__helm-intellisense-296441`, was missing
  from exp165's set** (no report, no log); it ran here with exit 0. **No bundle fails.**
- **Truth.** `census.py` labels units with exp096's scorer-14 truth exactly as `score.py` does (every frozen
  layer: chained, origins, copies2, misattr13, mapdefect14), and judges claims with `judge_origin`. **Label
  vocabulary found** (bun-dev, bytes): `<package>` 214,305,418; `<unmapped>` 16,219,535; `<self>` 10,929,584;
  `<app>` 40,982; no `<foreign>`. cc (sample units): `<app>` 511, library 108, `<compiler-helper>` 1. The app
  spelling is `"<app>"` — and on bun-dev the app's own code is overwhelmingly `"<self>"` (an installed CLI or
  extension maps into `node_modules/<its own package>`), so a census that only looks for app bytes must read
  BOTH. exp165's scripts compared against `"app"`, which matches neither (see "Corrections").
- **Causes.** Each missed library unit takes the first matching cause (`census.py` docstring): wrong package →
  no print → not indexed (stop-listed / all versions refused / never) → the right family holds prints covering ≥
  half the unit at the lookup's floors (then which guard: veto from the run's own `vetoed` list, then the
  structural rule emulated by exp095's `mmlib` with guards relaxed one at a time — window, tie, specificity,
  multi-package — and, where the emulation says the claim would stand, multi-family tie vs single-family) → only
  other families hold prints → every print under 20 nodes → the family holds the raw prints but under the lookup's
  30-node / 3-content floors → version not indexed (skew if another version holds the file) → print differs.
- **Attribution by measurement, not inference** (`toggles.py`): six bun-dev bundles holding 2.45 MB of the
  `guard.presence` bytes re-ran once per discovery guard switched off (11 configs + base, 72 runs under
  `/work/heavy.lock`).
- **Placement view.** For every factory, bundleOrder's `indexPath` (`vendor/<pkg>/…`) is where names mode places
  it (exp165's reading); a missed unit inside a factory placed under its right family is counted "placed right".

### Counters shown to fire before reading results

| counter                             | how it was shown to fire                                                                                     |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| app claimed (bun-dev)               | `--plant`: a fabricated claim on the first `<app>` unit of `cli_ember-cli` → `<app>:app` 18 B counted        |
| app claimed (cc)                    | `--plant` on the first `<app>` sample row → 1 sample unit counted                                            |
| `<self>` claimed as another package | fires naturally: 1.31 MB                                                                                     |
| every cause in the table            | each has ≥ 2 units and hand-read examples below; `not-indexed.refused` fired on node-releases (see its note) |
| named / wrong-family totals         | equal the official scorer's card sums to the byte                                                            |

## The causes, with examples (bun-dev code quoted)

**1. Not indexed — 7.47 MB, almost all long tail.** 48 packages. Weekly npm downloads (api.npmjs.org, 2026-10-23..29,
`notindexed-downloads.tsv`): **5.44 MB sit in 20 packages under 1,000 downloads/week**, each in ONE app (the
kryptokrona wallet stack 3.45 MB: kryptokrona-crypto 2.80 MB at 28 downloads/week; @liquify/specs 1.58 MB at 3/week);
1.03 MB in 17 packages under 100k/week; 0.27 MB in 3 under 1M; only **0.25 MB in 4 popular packages** (@workflow/builders
2.5M/week, bare-stream, @noble/post-quantum, bare-events) — plus the stop-listed helpers (tslib in 8 apps,
@babel/helpers, @babel/runtime: 0.42 MB, dropped from the index on purpose, exp129's helper table holds their prints)
and node-releases (252M/week, 67 KB: a JSON-data package the index owns no rows for; its one refusal is an
unpublished alpha, so "refused" is a misnomer — it is exp148's "versions that own no rows" lead).
Example: `kryptokrona-crypto` 1.0.3 (2.80 MB, 28 downloads/week) is bundled by one Electron wallet app, in both targets. Prior: exp150 `a` 3.5–3.6% (same
bytes: the scorer's own `a` here is 3.68 / 3.50%).

**2. Under the floors — 7.16 MB, but names mode already places 5.57 MB of it.**

- _sub-floor_ (3.61 MB, 102,854 units averaging 35 B): `wl.exports=c4` (micromark), `o9.isNode=An.isNode;` (yaml),
  `function Kc(e){this.enter({type:"delete",children:[]},e)}` (mdast-util-gfm-strikethrough, 17 nodes). 84% of the
  bytes sit in modules with NO claim at all, so DOWN/MODULE propagation has nothing to start from; the
  whole-module key places 2.30 MB of them anyway.
- _lookup floor_ (3.55 MB; 2.86 MB under 30 nodes, 0.69 MB whole prints with < 3 content tokens): the index HOLDS
  the print — `function Ete(e){var t=yte(e);if(t.length==1&&t[0][2])return Ate(t[0][0],t[0][1]);…}` (lodash
  `_baseMatches.js`, 49 nodes, 1 content token, its exact hash on the 4.18.1 row, also held by 32 other owners);
  `Object.defineProperty(q,"BorderHorizontalOutlined",{enumerable:!0,get:function(){return EJi.default}});`
  (@ant-design/icons, 20 nodes). Simulated "floors off" on unclaimed units: +2.78 MB right / 153 KB wrong (5.2%
  wrong); placement already places 3.27 MB of the 3.55. Prior: exp098 (the 20-node index floor is the lever),
  exp153 relax-floor (2.26 MB right / 2 KB wrong WITH call-context confirmation).

**3. Version skew — 2.68 MB [2.30 residual].** The bundled version is absent and no indexed version holds the
print. Of the 0.97 MB the examples cover: 0.64 MB same major as an indexed version (lucide-react 0.545.0 — indexed:
0.469, 0.562, 0.563, then 1.x; lucide releases weekly), 0.28 MB OLDER than every indexed version (eslint-plugin-jsdoc
50.8.0 vs 61+, tippy.js 2.6.0 vs 3+, i18n-iso-countries 3.7.8 vs 4.3.1+, azure-devops-node-api 11.2.0 vs 12.5+,
@vscode/vsce 3.9.2 / 2.32.0 vs only 4.0.0), 0.04 MB newer (vsce 2.15.0 vs 0.8.1/0.9.0). This is the pin rule
(year-last-12 + majors down to 4, exp152): apps pin old majors and fast-releasing packages fall between pins. No
skew byte is a refused version (all `refusedVersion=None`).

**4. A guard refused — 18.20 MB [10.58 residual].** Which guard, and was it right:

| guard                      | missed MB | residual | measured / simulated trade when relaxed                                                                                                                                                                                 | verdict                                                                                                                                                                                    |
| -------------------------- | --------: | -------: | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| isolation veto             |      7.05 |     2.04 | the run's own veto drops judged: **7.43 MB right/family/origin vs 72 KB wrong + 44 KB self-other**; `veto_support = "all-rules"` on 6 bundles: +923 KB right / 2.6 KB wrong                                             | **wrong on Bun**: ~1% of what it blocks is wrong; refuted elsewhere (exp143/144: remin2-terser P −0.069, remin4-esbuild app claimed)                                                       |
| factory whole-seed         |      4.04 |     4.02 | `factory_whole_seed = false` on 6 bundles: **+2.21 MB right (2.12 MB of it this class) / 33 KB wrong**, all the wrong bytes inside playwright-core's pre-bundled `utilsBundle.js` (named @vue/compiler-sfc, @mastra/\*) | **mostly wrong here**: it refuses big single-statement UMD/IIFE modules whose inner FUNCTION prints match — libsodium.js, xterm.js, sanitize-html, jquery, azure-devops-node-api's classes |
| multi-package (> 3 owners) |      2.40 |     1.12 | relaxed: 0.97 right + 1.37 family vs **0.96 MB wrong** (28%)                                                                                                                                                            | **right**: template code (@ant-design/icons' interop helpers held by 67 packages; Microsoft telemetry SDKs share code)                                                                     |
| held by another only       |      1.34 |     1.28 | (no relaxation can name the right family)                                                                                                                                                                               | carriers (playwright-core's utilsBundle held by @elevenlabs/elevenlabs-js) and truth map defects (`m5.exports={Aacute:"Á",…}` labelled domhandler, the entities table)                     |
| multi-family tie fenced    |      1.07 |     0.64 | `factory_tie_fence = false` on 6 bundles: +227 KB right / **38 KB wrong (14%)**: ms→debug, punycode→nodemailer, safe-buffer→readable-stream, encoding-sniffer→undici                                                    | **right**: an alphabetical tie-break between a dependency and its carrier                                                                                                                  |
| tie lost                   |      0.69 |     0.48 | (another family scores more)                                                                                                                                                                                            | color-name → color-convert, victory-vendor → @antv/vendor: shipper vs re-shipper                                                                                                           |
| position window            |      0.47 |     0.13 | base picks under 100 nodes, unclaimed: 0.58 MB right vs 0.23 MB wrong                                                                                                                                                   | **right** (28% wrong)                                                                                                                                                                      |
| specificity                |      0.12 |     0.12 | relaxed: 0.11 right vs **2.17 MB self-other** + 0.13 wrong                                                                                                                                                              | **right**                                                                                                                                                                                  |
| other single toggles       |         – |        – | `literal_conflict = false` +112 KB right / 0 wrong; `evidence_conflict = false` +23 KB origin/right; copy, context, shadow, payload, helper: ≤ 17 KB each                                                               | small                                                                                                                                                                                      |

Examples: veto — `class hM{constructor(H){this.config=H,…}create(H){…switch(H.type){case"sync"…` (tapable
`HookCodeFactory.js`, 6.8 KB, vetoed as tapable: a whole module whose neighbours are unclaimed);
`var yyr={icon:function(e,t){return{tag:"svg",attrs:{viewBox:"64 64 896 896",…` (@ant-design/icons-svg, one icon
per module, nothing calls a neighbour). Whole-seed — `(function(e){function t(r){var n=r,n;if(typeof n>"u")n={};…
sodium…` (libsodium 0.7.16 `dist/modules/libsodium.js`, 843 KB, 247,759 nodes covered); `(function(e,t){if(typeof
Ry=="object"&&typeof o1=="object")o1.exports=t();else if(typeof define=="function"&&define.amd)…` (@xterm/xterm 6.0.0).
Tie fenced — `VOr.exports=[[[0,44],2],[[45,46],2],…` (tr46 6.0.0 `lib/mappingTable.json`, 143 KB, held equally by
tr46 and whatwg-url).

**5. The print differs — 2.51 MB [2.33 residual].** The version (and in 1.59 MB the file) is indexed, the prints
are not. 75% of it is a handful of very large units: lucide-react's single ESM dist file (963 KB + 478 KB across 4 apps) and
terser (299 KB), playwright-core (151 KB) — **ESM tree-shaking**: Bun keeps the subset of a huge module the app
imports, so the statement is a different (smaller) tree than the index's build of the whole file. The rest is
small module-interop glue whose shape depends on what else the bundle contains: `var B_t=U_t(require("typescript")),
[j_t,Dqt]=B_t.versionMajorMinor.split(".").map(Number);` (typescript-eslint), `var ENt=[ZOt(),tNt(),iNt(),…];`
(encoding-sniffer's lazy-init list). Not decomposed further (it would need the index-side source of each build).

**6. Claimed as the wrong package — 384 KB (precision).** `seed` 288 KB, `down` 76 KB, `module` 19 KB. Top:
hyperdht-hugin claimed hyperdht (141 KB; a fork the family table does not join), string_decoder / safe-buffer
claimed readable-stream (46 KB: readable-stream ships them), @vscode/prompt-tsx claimed monaco-editor (36 KB),
rambdax → rambda (32 KB, a fork), playwright-core → @vue/compiler-sfc (24 KB, its utilsBundle). Two hand-read cases
are TRUTH defects, not discovery errors: `LDe.exports={name:"css-tree",version:"2.2.1",…}` is labelled csso and
claimed css-tree; `yPe.exports={name:"csso",…}` is labelled css-tree and claimed csso (Bun 1.4's map credits a
package.json to the neighbouring package; scorer 14's map-defect layer misses it).

**7. Other — 0.84 MB [0.53], hand-sampled 8 of 8.** The structural emulation says the right family holds prints,
but the run's own call-context rows read covered 0 for every family (spec-refused or held by > 3 owners), and with
every lookup guard relaxed the right family only ties with another one that sorts first: `terser` vs
`html-minifier-terser` (which bundles terser), `chalk` vs @antfu/ni / @parcel/utils / command-line-usage,
`supports-color` vs debug / jest-worker. Same mechanism as the tie rows; kept apart because it is inferred from
the sample, not counted.

**Remaining unexplained: 0.84 MB (0.39% of library bytes), the `guard.other` row**, explained by hand sample only.

## The reverse direction: app bytes discovery claims as library (claims only; exp166 owns vendor/ output)

- **bun-dev `<app>`: 0 of 40,982 B claimed** (the planted claim proves the counter counts). Placement puts 25 B
  of `<app>` under a vendor package (exp166's population).
- **bun-dev `<self>` (the app's own package): 10.93 MB; 1.60 MB claimed.** 290 KB as the app's own package
  (`ts-json-schema-generator`, `ts-patch`: judged `self`, correct). **1.31 MB as another package, all in two apps
  and none of it the app's own code** (hand-read): `cli_just-bash`'s pre-built `dist/` carries third-party
  libraries — re2js 682 KB, file-type / strtok3 / @tokenizer/inflate / modern-tar / @borewit/text-codec ~150 KB,
  named right by their own packages — and one pre-built esbuild chunk (`chunk-IMVXTSPG.js`, 461 KB) claimed as
  **npm-check-updates**, whose own dist carries the same chunk: a carrier mis-name, not app code. babel-istanbul's
  `istanbul` (1.6 KB) is the same kind.
- **`<unmapped>` claimed: 538 KB** (not judged): inlined package.json literals (`kK.exports={name:"eslint",
version:"8.57.1",…}`) and undici's/css-tree's map-less spans.
- **cc: 0 of 511 sampled app units claimed** (planted check fires).

## Comparison with earlier experiments

| claim / number                                     | earlier                                                                                                                          | here                                                                                                                                                                                                    |
| -------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| baseline recall / precision                        | exp153 /4 base: node P 99.742 R 82.702, bun P 99.825 R 81.593; cc 100.0 / 81.903                                                 | identical (names mode adds no claims; its gain is placement)                                                                                                                                            |
| miss classes (exp150, scorer `decompose.classify`) | `a` 3.5–3.6%, `c` 2.8–3.8%, `e-veto` 2.6–3.4%, `e-other` 1.6–3.2%, `e-ambig` 1.5–1.6%                                            | same reports read by the scorer: `a` 3.68/3.50, `c` 2.81/3.73, `e-veto` 3.38/2.84, `e-other` 3.18/1.57. **`e-other` is mostly the factory whole-seed rule** (4.04 MB, measured), plus multi-family ties |
| floor units                                        | exp098: the 20-node index floor is the lever; exp153 relax-floor +2.26 MB right / 2 KB wrong                                     | 7.16 MB under the floors, but **names mode's whole-module key already places 5.57 MB**: the floor lever is now worth 1.58 MB of placement                                                               |
| veto                                               | exp143/144: relaxing it fails remin2-terser and remin4-esbuild; exp153 veto drops 230/180 KB right per set (call-context subset) | on Bun it is nearly pure recall cost (7.43 MB right vs 116 KB wrong/self-other); placement recovers 5.01 MB of it                                                                                       |
| ties                                               | exp149: lost ties 989 KB vs 109 MB won, no vote rule net-positive; exp155 tie-break-down                                         | 1.76 MB in ties (fenced + lost) [1.12 residual]; turning the fence off is 14% wrong — the fence is right                                                                                                |
| version skew / shared helpers                      | exp153/155: innocent refutations are version skew and shared helpers                                                             | version absent 2.68 MB; stop-listed helper packages 0.42 MB; carriers ("held by other") 1.34 MB                                                                                                         |
| not indexed                                        | library-index §11: the rest of the misses is packages/versions the index lacks; exp148: 1,379 versions own no rows               | 7.47 MB, 73% of it in 20 packages under 1k downloads/week, each in one app                                                                                                                              |

### Corrections (rule 9: they live here, in the newer file)

- **exp165's app columns could never fire.** Its census scripts test `lab == "app"`; the truth spells `"<app>"`, and
  on bun-dev most of the app's own code is `"<self>"`. Its "ZERO app bytes" / "0.24 MB unmapped" columns therefore
  lumped `<app>`, `<self>` and `<unmapped>` together as "other". Re-measured here at the claim level: `<app>` claimed
  0 B; `<self>` claimed as another package 1.31 MB, every hand-read byte a vendored third-party library inside the
  app's own dist. exp165's conclusion (no app code inside claimed containers) survives; its numbers did not measure it.
- **exp165 ran 89 of 90 bun-dev bundles** (helm-intellisense `--target bun` has no report); its README says 89
  without naming the missing one.

## Recommended next levers (ranked by expected recall gain; none built)

Sizes are bun-dev bytes; "names-mode gain" is the residual a fix could recover after placement, "claims gain" is
the discovery-level gain (what identifier restoration and the scorer see).

| rank | lever                                                                                                                                                                                                                                                                | names-mode gain             | claims gain  | precision risk                                                                                                                                                                                  |
| ---: | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|    1 | **Factory whole-seed: accept function-print seeds that cover nearly all of a single-statement module** (UMD/IIFE wrappers: libsodium, xterm, jquery, sanitize-html, azure-devops-node-api). Keep the rule for multi-statement modules and pre-bundled carrier files. | **up to 4.0 MB (+1.9 pts)** | up to 4.0 MB | measured off-switch: 33 KB wrong per 2.21 MB right (1.5%), all in one carrier file (playwright's utilsBundle) — a coverage threshold should drop most of it. Judge on bun-held and a terser set |
|    2 | **Version coverage where apps actually are**: pin the versions dependency resolution picks (old majors still installed, dense pins for fast-releasing packages like lucide-react). Index-side, a delta build.                                                        | ~2.3 MB (+1.1 pts)          | ~2.7 MB      | low: the same package's own versions; watch new carriers                                                                                                                                        |
|    3 | **Isolation veto on Bun slices**: `all-rules` support or call-context support (exp153 `veto-or`) for Bun recipes only                                                                                                                                                | ~2.0 MB (+1.0 pt)           | up to 7.0 MB | low on Bun (72 KB wrong of 7.4 MB blocked); refuted on terser/esbuild judges (exp143/144), so it must be slice-scoped and re-judged there                                                       |
|    4 | **Print-differs from ESM tree-shaking**: let a statement match when its function prints are a SUBSET of one indexed statement's (lucide-react, terser)                                                                                                               | ~1.5–2.3 MB                 | same         | medium: subset matching is how carriers win; needs the same-file / same-version guard                                                                                                           |
|    5 | **Helper credit for stop-listed packages** (`helper_prints = "credit"` for whole tslib / @babel/helpers modules) and **JSON-only packages** (node-releases)                                                                                                          | 0.5 MB                      | 0.5 MB       | low                                                                                                                                                                                             |
|    6 | Index the ~0.5 MB of popular not-indexed packages (@workflow/builders, @openuidev/_, bare-_, @noble/post-quantum, inversify-binding-decorators); the other 5.4 MB is sub-1k-download long tail, not a generic lever                                                  | 0.5 MB                      | 0.5 MB       | low                                                                                                                                                                                             |
|    7 | Lookup floors 30 → 20 nodes / content 3 → 0 (exp153 relax-floor, call-context-confirmed)                                                                                                                                                                             | 0.27 MB                     | 2.8 MB       | 5% wrong unconfirmed; exp153's confirmed variant cost −0.011 pt P on node                                                                                                                       |
|    — | Do NOT relax: multi-package (28% wrong), tie fence (14% wrong), window (28% wrong), specificity (20× more self-other than right)                                                                                                                                     | —                           | —            | —                                                                                                                                                                                               |

Not levers: the long-tail not-indexed packages (one app each), "held by another" (carriers; exp148 refuted indexing
carriers) and truth map defects (fix the truth layer, not discovery).

## Files

- `census.py` — the cause census (bun-dev and cc), `--plant` self-test, `--selftest` one bundle.
- `toggles.py` — the guard attribution from the per-guard discover runs.
- `data/census-bundev.json` — bun-dev cause table: bytes, units, every package, mates, details, simulations,
  placement, reverse (bun-dev only: package names, byte counts, bundle ids).
- `data/census-cc.json` — cc: rates, sample counts and package names only (no code, no identifiers).
- `data/toggles.json`, `data/notindexed-downloads.tsv` — bun-dev counts.
- `data/scorer-summary-bundev.json` — the official scorer-14 summary of this label (its per-bundle cards sit in the
  gitignored `experiments/096-library-matching-score/results/exp167-names-cb64/`; label data dir
  `/work/exp150-data/bun-dev/frozen/results/exp167-names-cb64/`).
- Not committed: reports, examples with code (`/work/exp167-data/`), CC truth (`/work/cc-library-truth`).

Reproduce:

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python; C=experiments/167-library-miss-census
$PY -I $C/census.py bun-dev /work/exp167-data/all-reports /work/exp167-data/census-final
$PY -I $C/census.py cc /work/exp167-data/reports/cc-2.1.216.json /work/exp167-data/cc-final
$PY -I $C/toggles.py /work/exp167-data/toggles/reports /work/exp167-data/census-final/examples.json out.json
$PY -I $C/census.py bun-dev /work/exp167-data/all-reports /tmp/plant --plant   # the counters fire
```
