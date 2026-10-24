# exp143: the misses whose (package, version) the index holds — why they are missed, ranked by lever

## STATUS (2026-10-24)

DONE on branch `exp143-indexed-misses` (off main `541a82c5a`). **Census and simulation only: no crates change, no
default changed, no index built, nothing merged.** Dev corpora only (held-out by aggregate; test / test2 / remin2
never read).

- **The class.** exp142 found that 50-77% of missed library bytes on every dev set are units whose truth (package,
  version) IS in the index. This lane puts every one of those bytes (scorer 14, both scopes, main `673c72cc0`
  defaults, index `k2-all-r5-origin-prod`, labels `*exp142-base*`) into ONE mechanism. The per-set totals reproduce
  exp142's `held` column exactly (held-out 36,366 KB, pop-bun 44,745, apps-esbuild 20,043, ...).
- **The largest single cause on the non-re-minified sets is a GUARD, not the index: the isolation veto (R3).** The
  family holds the unit's print, the lookup accepts it, the seed is dropped because no graph neighbour is claimed by
  the same family (a highlight.js language file, an icon module, one big function of a CommonJS module whose
  `module.exports = f` sibling is too small to be a seed). Held-out 8.5 MB, pop 7.5 / 8.9 MB, apps-webpack 5.1,
  apps-esbuild 3.4, remin3-esbuild 7.6 MB; broad (thousands of units, top-5 units <= 12% of the bytes on held-out
  and pop). Judged seed by seed, **the veto drops 52.4 MB of seed bytes (not claimed by anything else) that would score RIGHT against 1.1 MB
  that would score wrong**, summed over the dev sets.
- **Measured (config only): `[library.discovery] veto_support = "all-rules"`** (let MODULE fills count as support,
  the lab prototype's rule). **F0.5 up on 12 of 15 dev sets (cc and dev-webpack unchanged, apps-rollup -0.001), precision up or equal on
  14**: held-out +1.29
  (P +0.007), esbuild-app +1.46, wrangler +1.24, apps-esbuild +0.70, pop +0.74 / +0.79, remin +1.58 / +2.37,
  remin3 +2.42 / +2.89. **The strict bar fails on two sets by one unit each:** apps-rollup P -0.040 (one 29 KB
  seed, `postcss-calc` claiming a `stylelint` unit) and remin3-esbuild app claimed +49 B. Andrew's call: the
  nearest thing to a flip in this campaign's recent arc; a judge look (test / test2 / remin2) would decide it.
- **Measured (config only): `[library.lookup] min_nodes = 20`** (the index's own unit floor; the lookup's is 30).
  P2 class, data and wasm units of 20-29 nodes whose whole print the family holds. F0.5 +0.03..+2.49 on 14 of 15
  sets, P up on 8; **fails** remin-terser P -0.227, remin3 -0.046 / -0.098, dev-rollup -0.029, wrangler -0.005, cc R
  -0.09. The new wrong is mostly PROPAGATED from the new seeds into known copy / fork classes (`yargs` <-
  `@curlconverter/yargs` 58 KB, `markdown-it` <- `entities` 36 KB), not the 20-29-node seeds themselves.
- **Both together (`exp143-combo`)**: the gains roughly add (held-out F0.5 +1.71, pop +1.21 / +1.07, remin
  +2.5 / +4.8, remin3 +3.2 / +5.2, esbuild-app +1.72) but the losses add too: 8 of 15 sets pass. `all-rules` alone
  is the cleaner candidate; `min_nodes = 20` needs its propagation fenced first.
- **Refuted / not levers at measurement:** `max_packages_by_family = true` (R1: +0.00..+0.12, P down on 6 sets),
  `lookup.support = 0` (R5: P down on 6 sets, held-out -0.021). Both rules earn their keep.
- **Simulated, not built: an isolation-veto exemption for an unambiguous seed holding >= half its module group**
  (nothing else the index accepts for it): on top of the right-scoring seed bytes the veto drops, it lets in
  0.0-11.2 KB of wrong per set except held-out (218 KB: ONE 205 KB unit, an index-side Bun map defect — tapable's
  `lib/Hook.js` row holds webpack's 210 KB schema validator). Sim F0.5 +0.04..+2.50, P up on 13 / 14. It reaches
  what `all-rules` cannot (apps-webpack: sim +2.50 vs measured +0.20 — big single functions of `typescript` /
  `typeorm` modules). Needs a crates change.
- **The re-minified sets are a different story:** their indexed misses are mostly the print differing (U2: remin3
  20-44 MB) and partial function cover (P3: 24-49 MB) — the minifier's transforms, the terser-slice / exp126 lane.
- **Index note (exp142 adopted after this census started):** the recommended index is now
  `k2-all-r5-origin-prod-asof20261015`. It adds only versions published after 2026-10-11, so every class here
  (versions the old index already held) is unchanged by construction; the ablations were not re-run on it. Its new
  releases add carrier candidates (exp142: pop -0.004..-0.006 P), which could add a few KB of wrong to any lever
  that frees more seeds. Re-score a flip candidate on the new index before deciding.

## 1. Method

`scripts/rows.py` (exp142's `misses.py` plus offsets, kind, module group, prints, the share of the unit's bytes the
truth's map gives its label, and what the run's report says: claim rule, vetoed seed, library-class closure,
queue head, the truth family's claims in the same module group and within the lookup's +-10 support window) emits
every missed unit whose truth version `versions.json` holds. `scripts/annotate.py` asks every index slice (the
bundle's own is the lookup's) what the truth FAMILY (package, its origin, `via`) holds for it, with the lookup's
rules (`LookupRules::recommended()`: whole print >= 30 nodes and 3 content tokens, else the greedy cover of
functions >= 30 nodes; specificity 4x + 4 KB; <= 3 packages per print; cover >= half the whole's nodes).
`scripts/summarize.py` puts each byte in the first matching class:

| class | meaning                                                                                                                                                                                                                                                                                                                           |
| ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| W     | claimed, as another family (precision-side; exp134 / 136 / 139's territory)                                                                                                                                                                                                                                                       |
| S     | scorer: the labelled package maps < 50% of the unit's bytes (straddle / glue)                                                                                                                                                                                                                                                     |
| N     | no print (a whole factory kept as one unit)                                                                                                                                                                                                                                                                                       |
| R1-R6 | the family holds a print the lookup ACCEPTS, refused downstream: R1 > 3 packages hold it, R2 tie or outscored by another family, R3 isolation veto (the report's `vetoed`), R4 helper table, R5 under 100 nodes and no family claim within +-10 units (support), R6 another discovery guard (presence, conflict, fences, payload) |
| P1-P4 | the family holds a print the lookup does NOT accept: P1 specificity, P2 whole print under 30 nodes, P3 functions cover < half, P4 functions too common / spec                                                                                                                                                                     |
| T     | tiny: no print >= 30 nodes and the family holds none (under 20 nodes the index stores no row at all)                                                                                                                                                                                                                              |
| U1-U5 | the family holds no print in this slice: U1 another family does, U2 the truth file has rows at the truth version (print differs), U3 rows at other versions only, U4 rows only in other slices, U5 the file is never indexed                                                                                                      |

Precision risk is measured where a config key exists (`scripts/score-all.sh` + `--pipeline-set`, every dev
corpus, then `scripts/compare.py` and `scripts/claimdiff.py`: per-unit gained / lost right and new wrong by rule,
size bucket and (claimed, truth) pair) and simulated where it does not (`scripts/vetsim.py` +
`scripts/vetclaimed.py` + `scripts/vettable.py`: every vetoed seed of every base label judged as if kept;
`scripts/ceiling.py` turns gained / wrong bytes into P / R / F0.5 against the base cards, no propagation).

## 2. The census (KB of missed bytes whose version the index holds)

| mechanism (KB)                                                                      |    heldout | esbuild-app | wrangler | apps-esbuild | apps-rollup | apps-webpack | dev-rollup | dev-webpack |    pop-bun | pop-esbuild | remin-esbuild | remin-terser | remin3-esbuild | remin3-terser |
| ----------------------------------------------------------------------------------- | ---------: | ----------: | -------: | -----------: | ----------: | -----------: | ---------: | ----------: | ---------: | ----------: | ------------: | -----------: | -------------: | ------------: |
| W claimed as another family                                                         |        426 |          77 |        6 |           85 |         582 |          155 |          2 |           4 |        450 |         421 |            89 |           16 |             77 |            34 |
| S scorer: unit straddles (< 50% mapped to label)                                    |      1,357 |           0 |        0 |            0 |          49 |          113 |          6 |          36 |      1,524 |           1 |             1 |            0 |              2 |             0 |
| N no print                                                                          |          0 |           0 |        0 |            0 |           0 |            0 |          0 |           3 |          0 |           0 |             0 |            0 |              0 |             1 |
| R1 too common (> 3 packages)                                                        |        751 |          15 |        2 |          673 |          59 |           56 |          0 |           1 |        521 |         460 |           447 |           92 |          1,100 |           166 |
| R2 tie / outscored by another family                                                |      2,125 |         109 |       74 |        2,223 |         207 |          745 |         28 |          12 |      1,832 |       1,466 |         2,181 |        1,034 |          1,809 |           774 |
| R3 isolation veto                                                                   |      8,493 |         850 |       12 |        3,369 |         381 |        5,122 |         17 |          26 |      7,452 |       8,882 |         2,171 |          542 |          7,586 |         1,448 |
| R4 helper table                                                                     |          1 |           0 |        0 |            0 |           0 |            0 |          0 |           0 |          0 |           0 |             0 |            0 |              0 |             0 |
| R5 weak and alone (support rule)                                                    |        231 |           7 |        8 |          321 |         108 |          100 |          8 |          11 |        139 |         145 |           153 |          222 |            281 |           546 |
| R6 other discovery guard (presence / conflict / fences)                             |      2,924 |       1,872 |       32 |        7,384 |         639 |           10 |          1 |           0 |     16,030 |      15,527 |           763 |          310 |          2,962 |         3,584 |
| P1 specificity (family holds it only in a much bigger module)                       |         13 |           0 |        0 |            3 |         261 |            1 |        289 |         222 |        483 |           3 |             2 |            1 |              3 |             2 |
| P2 below the lookup floor (whole print held, 20-29 nodes)                           |      3,631 |         112 |        6 |        2,762 |         169 |        1,333 |          5 |           7 |      5,185 |       2,173 |           979 |          573 |          1,385 |         1,652 |
| P3 partial: family functions cover < half                                           |        706 |          16 |      202 |          369 |       1,203 |          571 |         19 |           4 |      1,049 |       1,705 |        12,512 |       23,917 |         23,726 |        49,301 |
| P4 family functions too common / spec                                               |        134 |          15 |       30 |           46 |         131 |           78 |          1 |           9 |        164 |          32 |            86 |          492 |            207 |           909 |
| T tiny: no print >= 30 nodes, family holds none (< 20 nodes: under the index floor) |      3,730 |         288 |      239 |        2,407 |       2,350 |          776 |         82 |         142 |      6,474 |       6,887 |         4,570 |        5,263 |          5,804 |        11,271 |
| U1 only another family holds the print                                              |        316 |           0 |        2 |           65 |          17 |           15 |          3 |           1 |        234 |          51 |            24 |           62 |             45 |           128 |
| U2 truth file indexed at V, print differs                                           |     10,542 |          36 |      231 |           86 |       1,311 |          446 |          7 |         179 |      2,217 |       1,234 |         5,023 |       16,223 |         20,249 |        44,025 |
| U3 file indexed at other versions only                                              |        285 |           0 |        5 |           13 |         370 |           52 |          1 |           4 |        763 |         736 |           115 |          170 |            438 |           692 |
| U4 file indexed only in other slices                                                |        417 |           0 |       48 |          130 |         846 |          321 |          0 |           2 |         37 |         577 |           182 |          262 |            321 |           438 |
| U5 file never indexed                                                               |        284 |          24 |       12 |          107 |           9 |           43 |          0 |           0 |        192 |         617 |         3,337 |        6,611 |          4,474 |         7,352 |
| **total (version indexed, unit missed)**                                            | **36,366** |   **3,423** |  **909** |   **20,043** |   **8,692** |    **9,936** |    **470** |     **663** | **44,745** |  **40,917** |    **32,636** |   **55,790** |     **70,468** |   **122,321** |

Read with care:

- **held-out U2 (10.5 MB) is 9.0 MB truth**: two units of one bundle, 4.6 MB each, a data blob the truth's map
  credits to small source files (the Bun map defect exp136 / 137 found; scorer 14's rule does not catch these
  two). The rest of held-out U2 is ~1.3 MB.
- **R6 is concentrated**: on pop and apps-esbuild 90%+ of it is four or five units — `typescript.js` as shipped by
  `typescript` and inlined by `@ts-morph/common` (3.5-3.7 MB per unit). The pipeline sees a shipper / origin TIE;
  with every guard off the seed still falls to the isolation veto (an ambiguous seed cannot support itself). The
  scorer would accept either name. A tie lever was refuted in its claimed-unit form (exp134: per-unit carrier ->
  origin ownership breaks 1.7-50x more than it fixes); for UNCLAIMED ties it is unmeasured. esbuild-app's R6 is
  google-gax / logging-api generated protobuf modules (unambiguous; not diagnosed).
- **S** (held-out 1.4 MB, pop-bun 1.5 MB): units the scorer labels by a minority-share majority; not a pipeline
  lever.
- **T** is data: `@mdi/js` path constants, katex / entities / emoji-regex tables, undici's wasm strings (25-50%
  of T is literal-heavy, >= 16 bytes per node). The index stores no row under 20 nodes, so only propagation or a
  literal rule can reach it.

Top packages per class by KB (non-held-out sets; names only):

- **R3**: esbuild-app highlight.js 682, google-auth-library 92, google-gax 8, @supabase/realtime-js 7; wrangler @fastify/busboy 6, undici 4, execa 1, @iarna/toml 0; apps-esbuild highlight.js 682, @ant-design/icons-svg 667, element-plus 425, semver 113; apps-rollup vite 71, puppeteer-core 40, prettier 22, typeorm 22; apps-webpack typescript 2,877, typeorm 1,092, jpegxr 220, ajv 216; dev-rollup @codemirror/lang-css 6, ajv 2, just-diff 2, semver 1; dev-webpack semver 6, @codemirror/autocomplete 3, hosted-git-info 2, ajv 2; pop-bun naive-ui 2,519, highlight.js 1,418, lucide-vue-next 470, element-plus 341; pop-esbuild highlight.js 2,399, naive-ui 2,200, lucide-vue-next 472, element-plus 425; remin-esbuild cld3-asm 1,017, highlight.js 384, handlebars 54, @huggingface/transformers 54; remin-terser highlight.js 124, undici 40, eslint-visitor-keys 32, eslint-plugin-react 30; remin3-esbuild @icon-park/vue-next 2,809, highlight.js 1,536, @ant-design/icons-svg 665, naive-ui 578; remin3-terser highlight.js 495, undici 109, jsdom 94, @octokit/plugin-rest-endpoint-methods 59
- **R6**: esbuild-app google-gax 936, @google-cloud/logging-api 719, @anthropic-ai/claude-agent-sdk 121, gaxios 34; wrangler node-forge 10, resolve 7, undici 4, @colors/colors 4; apps-esbuild @ts-morph/common 3,487, typescript 2,963, prettier 246, jiti 182; apps-rollup xterm 539, tr46 69, lru-cache 6, file-entry-cache 4; apps-webpack @babel/plugin-transform-dynamic-import 1, @mermaid-js/parser 0, @babel/plugin-transform-duplicate-named-capturing-groups-regex 0, @babel/plugin-transform-unicode-property-regex 0; dev-rollup is-descriptor 0, define-property 0, @npmcli/git 0, micromark-factory-space 0; pop-bun typescript 7,168, @ts-morph/common 7,137, @modelcontextprotocol/client 433, jiti 363; pop-esbuild typescript 7,046, @ts-morph/common 7,017, naive-ui 458, jiti 370; remin-esbuild moment 128, @hot-loader/react-dom 64, semver 31, parse5 30; remin-terser azure-devops-node-api 49, iconv-lite 36, parse5 28, rxjs 24; remin3-esbuild prettier 460, yoga-layout-prebuilt 345, xgplayer 296, envinfo 143; remin3-terser @icon-park/vue-next 2,728, chardet 164, @ant-design/icons-svg 158, azure-devops-node-api 119
- **P2**: esbuild-app parse5 72, @opentelemetry/resources 5, get-east-asian-width 4, rxjs 3; wrangler debug 1, undici 1, is-fullwidth-code-point 0, prompts 0; apps-esbuild vm2 791, @material-ui/icons 587, @ant-design/icons 241, entities 187; apps-rollup lodash 22, prettier 14, core-js 9, typeorm 7; apps-webpack vm2 416, regenerate-unicode-properties 336, undici 141, parse5 72; dev-rollup tldts 1, semver 1, ajv 0, scule 0; dev-webpack semver 1, hookable 1, ip-address 0, source-map 0; pop-bun @shikijs/engine-oniguruma 1,823, undici 1,131, brotli 918, entities 332; pop-esbuild brotli 918, entities 324, @base-ui/react 145, regexpu-core 61; remin-esbuild vm2 396, parse5 76, regexpu-core 56, lodash 31; remin-terser parse5 79, regexpu-core 56, @babel/helper-validator-identifier 24, quicktype-core 18; remin3-esbuild encoding-japanese 198, @mui/icons-material 136, tldts-experimental 97, regexpu-core 56; remin3-terser @mui/icons-material 356, encoding-japanese 195, tldts-experimental 97, @icon-park/vue-next 72
- **T**: esbuild-app undici 142, emoji-regex 54, google-auth-library 15, highlight.js 9; wrangler undici 151, emoji-regex 25, @clack/core 16, yargs 5; apps-esbuild undici 566, katex 268, emoji-regex 145, entities 145; apps-rollup @material-ui/icons 1,357, katex 260, entities 127, @ant-design/icons 101; apps-webpack katex 259, entities 113, emoji-regex 68, caniuse-lite 37; dev-rollup tldts 67, web-streams-polyfill 5, @codemirror/view 2, semver 1; dev-webpack tldts 116, web-streams-polyfill 5, @codemirror/view 3, semver 2; pop-bun @mdi/js 2,057, katex 551, entities 497, @primeuix/styles 346; pop-esbuild @mdi/js 2,057, undici 1,277, katex 536, entities 493; remin-esbuild postman-sandbox 2,528, undici 582, entities 150, emoji-regex 109; remin-terser postman-sandbox 2,295, undici 590, vm2 419, entities 217; remin3-esbuild undici 1,602, entities 598, katex 545, emoji-regex 342; remin3-terser @mui/icons-material 4,324, undici 1,622, katex 673, entities 642
- **U2**: esbuild-app undici 14, @anthropic-ai/claude-agent-sdk 6, google-auth-library 3, html-to-text 3; wrangler undici 68, mime 35, zod 16, fast-xml-parser 14; apps-esbuild redux 20, prettier 11, @material-ui/core 9, antd 7; apps-rollup @material-ui/icons 549, caniuse-lite 222, prettier 117, @prisma/cli-engine 42; apps-webpack @ant-design/icons-svg 151, antd 84, @ant-design/icons 80, mermaid 27; dev-rollup hast-util-from-parse5 2, hastscript 1, vfile-message 1, tough-cookie 1; dev-webpack @lezer/javascript 75, @lezer/css 15, parse5 15, @codemirror/lang-jinja 15; pop-bun @vicons/ionicons5 1,015, terser 146, eslint-plugin-unicorn 137, eslint-plugin-n 102; pop-esbuild @vicons/ionicons5 1,015, shadcn 41, @shadcn/registry 29, systeminformation 19; remin-esbuild highlight.js 624, undici 446, semver 169, yaml 169; remin-terser azure-devops-node-api 2,016, undici 782, highlight.js 762, readable-stream 587; remin3-esbuild @faker-js/faker 2,607, highlight.js 2,501, jsdom 2,072, undici 1,243; remin3-terser azure-devops-node-api 4,887, highlight.js 3,046, @mui/icons-material 2,293, undici 2,149
- **P3**: esbuild-app google-auth-library 8, open 2, @inquirer/core 2, undici 2; wrangler yargs 51, undici 38, prompts 20, ws 17; apps-esbuild vite 191, antd 32, localforage 31, magicast 28; apps-rollup lodash 485, prettier 90, react-test-renderer 74, lodash.isequal 38; apps-webpack antd 102, file-type 52, rc-picker 38, mermaid 37; dev-rollup lodash.sortby 13, kind-of 2, cache-base 2, scule 1; dev-webpack ajv 2, index-to-position 1, is-stream 1, @whatwg-node/promise-helpers 0; pop-bun vite 460, prettier 142, @base-ui/react 108, eslint-plugin-regexp 52; pop-esbuild @base-ui/react 957, vite 452, naive-ui 68, eslint-plugin-import-x 39; remin-esbuild typescript 3,537, intl-relativeformat 963, chrome-remote-interface 684, quicktype-core 442; remin-terser typescript 3,431, azure-devops-node-api 2,318, cld3-asm 1,012, intl-relativeformat 788; remin3-esbuild typescript 3,508, jiti 1,650, lucide-react 1,128, playwright-core 936; remin3-terser typescript 5,873, azure-devops-node-api 5,679, jiti 1,613, elkjs 1,435

Upper-bound d F0.5 per class (every byte named, nothing wrong, no propagation; `ceiling.py`):

| class: upper-bound d F0.5 | heldout | esbuild-app | wrangler | apps-esbuild | apps-rollup | apps-webpack | dev-rollup | dev-webpack | pop-bun | pop-esbuild | remin-esbuild | remin-terser | remin3-esbuild | remin3-terser |
| ------------------------- | ------: | ----------: | -------: | -----------: | ----------: | -----------: | ---------: | ----------: | ------: | ----------: | ------------: | -----------: | -------------: | ------------: |
| R3                        |   +1.28 |       +1.42 |    +0.59 |        +0.66 |       +0.12 |        +2.13 |      +0.23 |       +0.57 |   +0.69 |       +0.80 |         +1.15 |        +0.89 |          +2.02 |         +1.29 |
| R6                        |   +0.45 |       +2.98 |    +1.58 |        +1.40 |       +0.19 |        +0.00 |      +0.02 |       +0.00 |   +1.45 |       +1.37 |         +0.41 |        +0.51 |          +0.81 |         +3.12 |
| P2                        |   +0.56 |       +0.19 |    +0.32 |        +0.54 |       +0.05 |        +0.58 |      +0.07 |       +0.16 |   +0.48 |       +0.20 |         +0.53 |        +0.94 |          +0.38 |         +1.47 |
| T                         |   +0.57 |       +0.50 |    +9.97 |        +0.47 |       +0.70 |        +0.34 |      +1.13 |       +2.97 |   +0.60 |       +0.62 |         +2.35 |        +7.79 |          +1.56 |         +8.99 |
| U2                        |   +1.57 |       +0.06 |    +9.71 |        +0.02 |       +0.39 |        +0.20 |      +0.09 |       +3.66 |   +0.21 |       +0.11 |         +2.57 |       +19.43 |          +4.99 |        +25.92 |
| P3                        |   +0.11 |       +0.03 |    +8.67 |        +0.07 |       +0.36 |        +0.25 |      +0.26 |       +0.09 |   +0.10 |       +0.16 |         +5.86 |       +25.27 |          +5.73 |        +27.86 |
| R2                        |   +0.33 |       +0.19 |    +3.56 |        +0.44 |       +0.06 |        +0.33 |      +0.39 |       +0.26 |   +0.17 |       +0.13 |         +1.16 |        +1.68 |          +0.50 |         +0.70 |
| R1                        |   +0.12 |       +0.03 |    +0.11 |        +0.13 |       +0.02 |        +0.03 |      +0.00 |       +0.02 |   +0.05 |       +0.04 |         +0.24 |        +0.15 |          +0.30 |         +0.15 |
| R5                        |   +0.04 |       +0.01 |    +0.42 |        +0.06 |       +0.03 |        +0.04 |      +0.12 |       +0.24 |   +0.01 |       +0.01 |         +0.08 |        +0.37 |          +0.08 |         +0.49 |

## 3. The isolation veto, seed by seed (R3)

`vetsim.py`: every seed a base report lists as vetoed, judged as if kept, and not claimed by anything else in the
base run (`vetclaimed.py`). Ambiguous = another family's prints also reach the lookup's cover for the unit. Cells:
right KB / wrong KB (simulated d P / d F0.5); `data/vetoed-seeds.json`.

| set            |    all vetoed right / wrong KB |   unambiguous right / wrong KB | unambiguous, exact whole print right / wrong KB | unambiguous, factory body right / wrong KB | unambiguous, >= half its module right / wrong KB | unambiguous, exact, >= half its module right / wrong KB | unambiguous, exact, >= 1 KB right / wrong KB |
| -------------- | -----------------------------: | -----------------------------: | ----------------------------------------------: | -----------------------------------------: | -----------------------------------------------: | ------------------------------------------------------: | -------------------------------------------: |
| apps-esbuild   |  3,476 / 19.7 (-0.014 / +0.66) |   3,459 / 3.5 (-0.000 / +0.67) |                    3,433 / 3.5 (-0.000 / +0.67) |               3,437 / 3.5 (-0.000 / +0.67) |                     3,355 / 0.0 (+0.003 / +0.65) |                            3,336 / 0.0 (+0.003 / +0.65) |                 2,271 / 0.0 (+0.002 / +0.45) |
| apps-rollup    |    550 / 81.3 (-0.098 / +0.09) |    387 / 22.3 (-0.024 / +0.10) |                     309 / 18.3 (-0.020 / +0.08) |                                          - |                       137 / 0.0 (+0.002 / +0.04) |                               93 / 0.0 (+0.001 / +0.03) |                   187 / 5.4 (-0.005 / +0.05) |
| apps-webpack   | 7,146 / 157.1 (-0.210 / +2.71) |   6,300 / 5.0 (+0.034 / +2.57) |                    2,688 / 4.0 (+0.012 / +1.15) |                                          - |                     6,099 / 0.0 (+0.041 / +2.50) |                            2,592 / 0.0 (+0.018 / +1.11) |                 2,381 / 0.0 (+0.017 / +1.03) |
| dev-rollup     |      24 / 1.4 (-0.083 / +0.28) |      17 / 0.0 (+0.001 / +0.24) |                       12 / 0.0 (+0.001 / +0.18) |                                          - |                         4 / 0.0 (+0.000 / +0.05) |                                3 / 0.0 (+0.000 / +0.05) |                     6 / 0.0 (+0.001 / +0.09) |
| dev-webpack    |      39 / 0.0 (+0.012 / +0.86) |      27 / 0.0 (+0.008 / +0.61) |                       22 / 0.0 (+0.007 / +0.50) |                                          - |                        18 / 0.0 (+0.006 / +0.40) |                               15 / 0.0 (+0.005 / +0.33) |                    16 / 0.0 (+0.005 / +0.35) |
| esbuild-app    |     862 / 4.5 (+0.024 / +1.42) |     856 / 1.2 (+0.046 / +1.43) |                      846 / 1.1 (+0.046 / +1.41) |                 846 / 0.0 (+0.054 / +1.42) |                       838 / 0.0 (+0.053 / +1.40) |                              830 / 0.0 (+0.053 / +1.39) |                   761 / 0.0 (+0.049 / +1.28) |
| heldout        | 9,028 / 288.1 (-0.152 / +1.24) | 8,798 / 241.3 (-0.125 / +1.22) |                  8,754 / 241.1 (-0.125 / +1.22) |             8,706 / 237.0 (-0.122 / +1.21) |                   7,886 / 218.3 (-0.114 / +1.10) |                          7,865 / 218.3 (-0.114 / +1.09) |               7,646 / 225.9 (-0.119 / +1.06) |
| pop-bun        | 7,886 / 277.2 (-0.093 / +0.65) |  7,838 / 83.2 (-0.018 / +0.70) |                   7,759 / 66.6 (-0.011 / +0.70) |              7,647 / 36.6 (+0.000 / +0.70) |                    7,364 / 11.2 (+0.010 / +0.68) |                           7,329 / 10.9 (+0.010 / +0.68) |                6,366 / 31.8 (-0.000 / +0.58) |
| pop-esbuild    | 9,486 / 147.5 (-0.031 / +0.81) |  9,430 / 68.2 (-0.001 / +0.83) |                   9,370 / 62.6 (+0.001 / +0.82) |              8,581 / 44.2 (+0.006 / +0.76) |                     8,307 / 1.3 (+0.021 / +0.75) |                            8,271 / 1.3 (+0.021 / +0.74) |                7,707 / 38.4 (+0.006 / +0.68) |
| remin-esbuild  |   2,722 / 0.0 (+0.014 / +1.44) |   2,702 / 0.0 (+0.014 / +1.43) |                    2,659 / 0.0 (+0.014 / +1.40) |               2,642 / 0.0 (+0.014 / +1.39) |                     2,362 / 0.0 (+0.013 / +1.25) |                            2,351 / 0.0 (+0.013 / +1.25) |                 2,181 / 0.0 (+0.012 / +1.16) |
| remin-terser   |     715 / 2.8 (-0.011 / +1.17) |     684 / 1.6 (-0.005 / +1.12) |                      659 / 1.6 (-0.005 / +1.08) |                 681 / 1.6 (-0.005 / +1.11) |                       329 / 0.0 (+0.002 / +0.54) |                              326 / 0.0 (+0.002 / +0.54) |                   311 / 0.0 (+0.002 / +0.52) |
| remin3-esbuild |  8,721 / 90.3 (-0.057 / +2.25) |  8,659 / 82.2 (-0.050 / +2.24) |                    8,579 / 3.2 (+0.022 / +2.27) |              8,623 / 82.1 (-0.050 / +2.23) |                     8,151 / 0.0 (+0.024 / +2.16) |                            8,128 / 0.0 (+0.024 / +2.15) |                 6,192 / 0.0 (+0.019 / +1.66) |
| remin3-terser  |   1,690 / 6.5 (-0.012 / +1.50) |   1,663 / 3.6 (-0.003 / +1.48) |                    1,624 / 3.6 (-0.004 / +1.44) |               1,654 / 3.0 (-0.002 / +1.47) |                     1,070 / 0.0 (+0.005 / +0.96) |                            1,066 / 0.0 (+0.004 / +0.95) |                 1,035 / 0.0 (+0.004 / +0.93) |
| wrangler       |      18 / 4.9 (-0.747 / +0.56) |      17 / 0.0 (+0.025 / +0.85) |                       11 / 0.0 (+0.016 / +0.54) |                  11 / 0.0 (+0.016 / +0.55) |                         8 / 0.0 (+0.012 / +0.39) |                                8 / 0.0 (+0.012 / +0.39) |                     7 / 0.0 (+0.011 / +0.37) |

The veto's purpose (docs/design/library-discovery.md §7 guard 4) is a FORK: Claude Code 2.1.86's ink fork kept a
library's leaf tables and rewrote the code around them. On these corpora that case is rare and the cost is not:
"costs ~2.5 KB of real library claims per release" was measured on one app; it is 0.8-9.2 MB per dev set.

## 4. Measured relaxations (config only, every dev corpus)

Labels `*exp143-<name>*` (main `673c72cc0` binary, `k2-all-r5-origin-prod`, structural, scorer 14). Bar: P >= base,
F0.5 >= base - 0.002, app claimed <= base. d P / d F0.5 in points.

| set                  |       min_nodes = 20 | veto_support = "all-rules" |                 both | max_packages_by_family |          support = 0 |
| -------------------- | -------------------: | -------------------------: | -------------------: | ---------------------: | -------------------: |
| cc                   | +0.000 / -0.03 **x** |             +0.000 / +0.00 | +0.000 / -0.03 **x** |         +0.000 / +0.03 |       +0.000 / +0.09 |
| esbuild-app          |       +0.008 / +0.30 |             +0.055 / +1.45 |       +0.062 / +1.72 |         +0.000 / +0.01 |       +0.001 / +0.02 |
| heldout              |       +0.003 / +0.56 |             +0.007 / +1.29 |       +0.009 / +1.71 |   -0.002 / +0.02 **x** | -0.021 / +0.05 **x** |
| wrangler             | -0.005 / +1.36 **x** |             +0.037 / +1.24 |       +0.026 / +2.40 |         +0.004 / +0.12 |       +0.019 / +1.36 |
| apps-esbuild         |       +0.001 / +0.41 |             +0.002 / +0.70 | -0.001 / +1.08 **x** |   -0.003 / +0.01 **x** |       +0.001 / +0.05 |
| apps-rollup          | -0.000 / +0.03 **x** |       -0.040 / -0.00 **x** | -0.042 / +0.03 **x** |   -0.001 / +0.03 **x** | -0.007 / +0.06 **x** |
| apps-webpack         |       +0.007 / +0.43 |             +0.003 / +0.20 |       +0.010 / +0.62 |         +0.001 / +0.02 |       +0.001 / +0.07 |
| dev-rollup           | -0.029 / +0.08 **x** |             +0.000 / +0.06 | -0.029 / +0.12 **x** |         +0.000 / +0.00 |       +0.008 / +0.13 |
| dev-webpack          |       +0.005 / +0.34 |             +0.000 / +0.00 |       +0.005 / +0.34 |   -0.000 / -0.03 **x** |       +0.003 / +0.29 |
| pop-bun              |       +0.002 / +0.49 |             +0.006 / +0.74 |       +0.008 / +1.21 |   -0.007 / +0.00 **x** |       +0.002 / +0.20 |
| pop-esbuild          |       +0.006 / +0.30 |             +0.019 / +0.79 |       +0.026 / +1.07 |         +0.001 / +0.01 |       +0.011 / +0.03 |
| remin-esbuild        |       +0.012 / +1.28 |             +0.015 / +1.58 |       +0.024 / +2.53 |   -0.018 / +0.02 **x** | -0.007 / +0.26 **x** |
| remin-terser         | -0.227 / +2.49 **x** |             +0.007 / +2.37 | -0.204 / +4.76 **x** |         +0.000 / +0.01 | -0.077 / +2.14 **x** |
| remin3-esbuild       | -0.046 / +1.17 **x** |       +0.009 / +2.42 **x** | -0.015 / +3.20 **x** |   -0.002 / +0.05 **x** | -0.006 / +0.18 **x** |
| remin3-terser        | -0.098 / +2.49 **x** |             +0.012 / +2.89 | -0.078 / +5.23 **x** |   +0.003 / -0.19 **x** | -0.052 / +0.15 **x** |
| sets passing the bar |               8 / 15 |                    13 / 15 |               8 / 15 |                 7 / 15 |               9 / 15 |

(**x** = fails the bar; base `*exp142-base*`; full rows in `data/ablation-*.json`.)

Where the new wrong came from (`claimdiff.py`, `data/claimdiff-*.json`):

- `veto_support = "all-rules"`: apps-rollup's +31.9 KB is one seed (`postcss-calc` <- `stylelint`, 29 KB); held-out
  +20 KB (14 KB seeds >= 100 nodes); pop-bun +24 KB (`css-tree` / `stylelint` <- `lru-cache`, a vendored copy);
  remin3-esbuild +30 KB (`yargs` <- `@curlconverter/yargs`, the fork exp141 found) and +49 B app.
- `min_nodes = 20`: remin-terser +51 KB (36 KB `markdown-it` <- `entities` by MODULE fill), remin3 +63 / +41 KB
  (`yargs` fork 58 / 22 KB, `@dual-bundle/import-meta-resolve` <- `@babel/core`), pop-bun +36 KB (`file-set` <-
  `lru-cache` 16 KB by DOWN). Seeds of 20-29 nodes themselves: <= 6.3 KB per set.

## 5. Ranked levers

| rank | mechanism (class)                                                                                           | missed KB: held-out / pop-bun / pop-esb / apps-esb / apps-wp / remin3-esb | measured or sim d F0.5                                                                  | precision risk                                                                              | suggested generic fix                                                                                                                                                                                                                                 | est. cost                                                  |
| ---: | ----------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
|    1 | isolation veto drops seeds whose family holds the print (R3)                                                | 8,493 / 7,452 / 8,882 / 3,369 / 5,122 / 7,586                             | MEASURED `veto_support = "all-rules"`: +0.06..+2.89 on 12 / 15, P up or equal on 14     | apps-rollup -0.040 (one 29 KB seed), remin3-esb +49 B app; held-out +20 KB wrong            | flip `veto_support = "all-rules"` after a judge look and a re-score on the as-of-10-15 index; then the exemption below                                                                                                                                | config flip + gate + neutrality (~4 h incl. judge)         |
|    2 | same, the part `all-rules` cannot reach: an unambiguous seed holding >= half its module (R3)                | (subset of the above)                                                     | SIM seed bytes only: +0.04..+2.50 (apps-webpack +2.50, held-out +1.10)                  | 0-11 KB wrong per set; held-out 218 KB = one index-side Bun map defect (205 KB)             | veto exemption: an index seed that no other family's prints accept and that holds >= half its module group (factory body / lazy module) is its own support; plus a builder sanity check (row moduleBytes vs the file's installed size) for the defect | crates change (discovery.rs `isolated`) + test; ~1 day     |
|    3 | lookup floor 30 nodes > index floor 20 (P2)                                                                 | 3,631 / 5,185 / 2,173 / 2,762 / 1,333 / 1,385                             | MEASURED `min_nodes = 20`: +0.03..+2.49 on 14 / 15, P up on 8                           | new wrong is propagation into copies / forks (remin-terser -0.227, remin3 -0.05 / -0.10)    | lower the floor for SEEDS only, or require family support for propagation from a sub-30-node seed; re-measure                                                                                                                                         | crates change (a seed-only floor key) ~0.5 day + measure   |
|    4 | shipper / origin tie the guards fence, then the veto drops (R6, mostly `typescript` <-> `@ts-morph/common`) | 2,924 / 16,030 / 15,527 / 7,384 / 10 / 2,962                              | upper bound +0.45 / +1.45 / +1.37 / +1.40 (CONCENTRATED: 4-5 units per set)             | exp134 refuted carrier -> origin ownership for claimed units; unclaimed ties unmeasured     | for an unclaimed tie between a package and the package its dist inlines, name the one whose index module is the smaller context (the origin), else nothing; simulate on claimed-tie truth first                                                       | sim ~0.5 day; crates ~1 day                                |
|    5 | tiny data units under the index's 20-node floor (T)                                                         | 3,730 / 6,474 / 6,887 / 2,407 / 776 / 5,804                               | upper bound +0.57 / +0.60 / +0.62 / +0.47 / +0.34 / +1.56                               | data tables are often copied (entities' decode data in parse5 / entities forks): unmeasured | a literal seed (the unit's long literal held only by the family's files, `literals.bin`) or an index floor by bytes for literal-heavy units                                                                                                           | literal rule: crates ~1-2 days; index floor: rebuild ~18 h |
|    6 | the truth file is indexed at the version, the print differs (U2)                                            | 10,542 (9,000 truth) / 2,217 / 1,234 / 86 / 446 / 20,249                  | upper bound +1.57 held-out (mostly truth), +0.21 pop; remin3 +5.0 / terser +25.9        | -                                                                                           | non-remin: per-package build-shape (exp136's `static`, not recommended); remin: the terser-slice / normalisation lane (exp126 / 139 / 141)                                                                                                            | lane-specific                                              |
|    7 | partial function cover (P3)                                                                                 | 706 / 1,049 / 1,705 / 369 / 571 / 23,726                                  | upper bound <= +0.36 off remin; remin +5.7..+27.9                                       | -                                                                                           | re-minification: same as U2 (remin only)                                                                                                                                                                                                              | remin lane                                                 |
|    8 | tie / outscored by another family (R2)                                                                      | 2,125 / 1,832 / 1,466 / 2,223 / 745 / 1,809                               | upper bound +0.13..+0.44 off remin                                                      | carriers (exp133 / 134 / 139)                                                               | carrier lane                                                                                                                                                                                                                                          | -                                                          |
|    9 | too common (R1) / weak and alone (R5)                                                                       | 751 / 521 / 460 / 673 / 56 / 1,100 ; 231 / 139 / 145 / 321 / 100 / 281    | MEASURED: `max_packages_by_family` +0.00..+0.12, P down on 6; `support = 0` P down on 6 | measured: both rules pay for themselves                                                     | none                                                                                                                                                                                                                                                  | -                                                          |
|    - | scorer straddle (S), wrong-family claims (W)                                                                | 1,357 / 1,524 / 1 / 0 / 113 / 2 ; 426 / 450 / 421 / 85 / 155 / 77         | -                                                                                       | -                                                                                           | scorer / precision lanes                                                                                                                                                                                                                              | -                                                          |

## 6. Files

- `scripts/rows.py`, `annotate.py`, `summarize.py`, `tables.py`: the census (rows and annotations stay in
  `/work/exp143-data`, they name bundles).
- `scripts/vetsim.py`, `vetclaimed.py`, `vettable.py`, `ceiling.py`: the veto simulation.
- `scripts/score-all.sh`, `compare.py`, `claimdiff.py`: the ablations.
- `data/census.json` (KB per class / sub-class, unit counts, concentration; top packages for non-held-out sets),
  `data/vetoed-seeds.json`, `data/ablation-*.json`, `data/claimdiff-*.json`.
- Labels `exp143-{mn20,vall,mpf,sup0,combo}` (+ `-apps`, `-bundlers`, `pop-`, `remin-`, `remin3-`): cards and
  summaries under `experiments/096-library-matching-score/results/`.

## 7. Gate

`npm run check` (bare, `/work/exp143-data/gate.log`): exit 0, ALL 13 STAGES PASSED on the tree committed with
this README.
