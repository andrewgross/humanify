# exp137: scorer 14 — the truth's Bun source-map defect, corrected from installed code

## STATUS (2026-10-22)

DONE on branch `exp137-scorer-mapdefect` (off main `869931920`). **Measurement change only: no crates change, no
pipeline change, no neutrality needed.** Scorer 14 = scorer 13 + rule `mapdefect`; all **439** committed labels
re-scored once.

- **Rule** (section 1): a unit whose labelled package's INSTALLED closure (the package at the map's path plus every
  dependency resolved from it, transitively) holds **under 20%** of its distinctive name evidence (at least 8 tokens,
  each held by at most 5 installed packages), while **one installed file** of exactly **one other family** holds
  **at least 80%**, takes that package. Evidence: the app's own install tree, the frozen bundle / map / truth — never
  the index or a claim. Claim-blind: it moves claims both ways and relabels unclaimed units (recall's truth) alike.
- **Audit** (section 3, dev only): **all 104** relabelled units in the default labels' dev units judged by hand
  (unit beside both packages' installed files; the seeded stratified draw of 42 first, then the other 62):
  **0 wrong (0/104; 0/42 in the draw)** — encoding tables to iconv-lite, spinners to cli-spinners, node release
  data to node-releases, socks constants, css-tree / cssstyle modules, inlined package.json files, ... (42 distinct
  label -> package pairs). **48 wrong-family claims the rule left alone** (24 any size, 24 of >= 1 KB): **0 obvious
  misses** — the label's closure holds the code (copies / carriers), too little evidence, no install tree, outside the
  rule, or (one) two families hold it.
- **Effect, main's defaults on the recommended index** (`*exp135-prod*`, k2-all-r5-origin-prod, both scopes;
  section 4): precision up or equal on **every** set, never down; held-out **99.56 -> 99.64** (F0.5 91.29 -> 91.35),
  pop-bun 99.49 -> 99.52, test-bun +0.006, test2-bun 99.26 -> 99.36, test2-rollup 96.35 -> 96.44; non-Bun sets move by
  thousandths or not at all; app claimed 0 unchanged. **Wrong -> right 421.6 KB (+5.2 KB wrong -> same family), right -> wrong 1.7 KB** over all sets
  (held-out 113.9 / 0.3 KB, pop-bun 59.0 / 0.4, test2-bun 170.1 / 0.8).
- **Smaller than exp136's audit** (held-out 314 KB, audited P 99.76; here 114 KB, 99.64): exp136 counted a unit as
  the claimer's when the label's package held under 50% and the claimer's package held 50%, all files pooled, from the
  INDEX's work installs. This rule wants under 20% vs one file holding 80%, a single owning family, and the APP's own
  install. What exp136 counted beyond it sits in "label holds" (a dependency of the label, or 20-50%) and "too little
  evidence"; the 48-claim audit found no clear miss among them, so the gap is the stricter bar, not missed defects.
- **All 439 labels**: 1,277 label-sets; 915 identical, 348 moved with precision up or equal, 14 down (largest
  -0.018 points: two exp128 test-bun labels, aggregates only; the rest -0.006 to -0.007 on twelve older apps-rollup labels, where a
  claim names a package the relabel shows is not the unit's owner).
- Gate: 13/13 bare at the commit (`npm run check`).

## 1. The rule (`mapdefect`, frozen layer `mapdefect14`)

exp136 found that many "wrong-family" claims are right: the bundle's source map (Bun 1.4) credits a unit to a
package whose installed files do not hold its code, while the claimed package's do (a 42 KB encoding table in three
apps labelled another package's `index.js`). Scorer 13 (exp132) corrects one shape of it — a module whose file is
also another module's majority source. Scorer 14 reads every judged unit's own evidence against the app's install
tree, whatever the map's shape.

Evidence is **independent of the matcher**: the frozen bundle, map and truth, the app's OWN install tree (the
`node_modules` the map's sources live in — the corpus's install, as scorer 12's copies layer reads it), and which
units exist (the unit spans of two reference runs per corpus: today's defaults `*exp135-prod*` and the
factory-off `*exp130-origin*`; the unit model, the same for every label of one binary). No index row, no claim, no
score is read, so the rule moves claims right -> wrong as readily as wrong -> right, and relabels unclaimed units
(recall's truth) the same way.

Per judged unit U (statement, lazy body, factory, factory body; bundler runtime excluded) whose scorer-13 label is a
library package T with no copy origin and no scorer-13 relabel:

| step            | test                                                                                                                                                                                                                                                    | constant  |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| evidence        | U's name evidence, read by acorn (`tokens14.mjs`): string / template values, `.property` names, object keys of 4+ characters; verbatim-searchable, less a stoplist of names every file holds (exp136's)                                                 |           |
| distinctive     | a token held by at most `DF_MAX` installed packages of the app (by name), in at most `FILE_CAP` files                                                                                                                                                   | 5 / 1,000 |
| enough          | at least `MIN_EVIDENCE` distinctive tokens (more than `SAMPLE`: the first by sha1, content-blind)                                                                                                                                                       | 8 (64)    |
| (a) label lacks | T's installed CLOSURE — T's install root on the map's path plus every dependency / optional / peer dependency resolved from it as node does (nested, hoisted, pnpm siblings), transitively, all files together — holds under `LACK` of them (substring) | 20%       |
| (b) other holds | ONE installed file of a package outside that closure holds at least `HOLD` of them (substring)                                                                                                                                                          | 80%       |
| unique          | every such file belongs to one family (families_lib), not T's (another install of T's family holding it = T's family holds it), not the app's own package                                                                                               |           |

Then U takes that package (the file holding the most; version from its `package.json`). Anything else — T not
installed at the map's path, too little evidence, T's closure holding 20% or more, nobody holding 80%, two families
— leaves the label. A token-set prefilter (exact values, which imply the substring test) settles "T holds it"
without reading text; only the rest are checked by substring.

Code: `scorer14/mapdefect14.py` (layer builder; `decide`, `closure`, `distinctive` are the rule),
`scorer14/tokens14.mjs`, `truth_audit.Audit.mapdefect` + `score.label_units` (applied by span after scorer 13's
rule; the unit's copy origin is then read for the new shipper), `SCORER_VERSION` `exp096-score/14`, summary config
`mapdefect14Sha256`; layers `<DATA>/truth/<id>.mapdefect14.json.gz`, shas and per-set verdict bytes in
`096-library-matching-score/mapdefect14[-<corpus>].json`. A corpus scored under scorer 14 needs its layer
(`mapdefect14.py build --corpus C --units <reference labels>`); `score.py` refuses a layer frozen for another
corpus manifest.

**Tests** (`test/library-truth-mapdefect14.test.ts`, run by `npm run check`'s unit stage): the verdict (relabel; label
holds at exactly 20%; seven tokens; two families; another install of the label's family; the app's package; best
file of one package), distinctive tokens, the closure (nested, hoisted, optional, peer, pnpm sibling, transitive, not
an unrelated package), and at score time both directions (a claim naming the holder turns right, one naming the old
label wrong, an unrelated unit untouched) plus rule off = scorer 13. The score-time tests were watched red before
the hook; the rule's functions were written first, so each of their tests was checked by mutation instead (nine
mutants — `>=` -> `>` on LACK, no peer/optional dependencies, no family check, no ambiguity check, MIN_EVIDENCE 3,
no df filter, no app check, best-by-size, no ancestor walk — each fails exactly one test).

**Rule off = scorer 13** (`verify_s13.py`): `exp135-prod`, `pop-exp135-prod`, `exp135-prod-bundlers` re-scored with
the rule off differ from their scorer-13 cards and summaries in the schema string only (191 files; then re-scored with the rule on).

## 2. What the layer relabels (both directions)

Ledger (`scorer14/ledger14.py`) on the default labels, claimed bytes by verdict rule off -> on, plus what it does
to unclaimed units (recall's truth: library -> library, so recall's denominator never moves; the miss classes of
those units now read the new package). test / test2: aggregates only, never inspected (`data/ledger-main.json`).

| set                                                                        | units relabelled | relabelled KB | of which unclaimed (recall truth) | wrong -> right | wrong -> family | right -> wrong | wrong -> wrong (third package) | library bytes the layer read |
| -------------------------------------------------------------------------- | ---------------: | ------------: | --------------------------------: | -------------: | --------------: | -------------: | -----------------------------: | ---------------------------: |
| heldout                                                                    |               48 |         863.3 |                             744.8 |          113.9 |             1.1 |            0.3 |                            3.2 |                        96.8% |
| apps-esbuild                                                               |                6 |           3.2 |                               0.0 |            3.2 |             0.0 |            0.0 |                            0.0 |                        96.0% |
| apps-rollup                                                                |                4 |          36.2 |                              36.2 |            0.0 |             0.0 |            0.0 |                            0.0 |                        97.8% |
| pop-bun                                                                    |               41 |         636.4 |                             577.0 |           59.0 |             0.0 |            0.4 |                            0.0 |                        88.4% |
| pop-esbuild                                                                |                4 |           4.2 |                               0.0 |            4.2 |             0.0 |            0.0 |                            0.0 |                        89.3% |
| remin-esbuild                                                              |                1 |           0.3 |                               0.3 |            0.0 |             0.0 |            0.0 |                            0.0 |                        99.3% |
| test-bun                                                                   |               38 |         465.1 |                             450.9 |           13.5 |             0.0 |            0.3 |                            0.5 |                        88.2% |
| test-esbuild                                                               |                8 |           5.4 |                               0.0 |            5.4 |             0.0 |            0.0 |                            0.0 |                        89.8% |
| test-rollup                                                                |                1 |          33.4 |                              33.4 |            0.0 |             0.0 |            0.0 |                            0.0 |                        78.0% |
| test-webpack                                                               |                1 |           4.6 |                               4.6 |            0.0 |             0.0 |            0.0 |                            0.0 |                        89.5% |
| test2-bun                                                                  |               97 |         932.3 |                             756.0 |          170.1 |             0.0 |            0.8 |                            5.5 |                        94.8% |
| test2-esbuild                                                              |                4 |           4.2 |                               0.0 |            4.2 |             0.0 |            0.0 |                            0.0 |                        97.3% |
| test2-rollup                                                               |               40 |          67.5 |                              25.0 |           38.5 |             4.1 |            0.0 |                            0.0 |                        92.0% |
| test2-webpack                                                              |               17 |          33.1 |                              23.5 |            9.6 |             0.0 |            0.0 |                            0.0 |                        96.4% |
| esbuild-app, wrangler, apps-webpack, dev-rollup, dev-webpack, remin-terser |                0 |               |                                   |                |                 |                |                                |                   97.8-99.9% |

"Library bytes the layer read": the rest is units the rule does not judge by design — a unit with a copy origin
(a claim naming the origin is already right) or one scorer 13 relabelled — plus (wrangler) no install tree. The
relabelled bytes are mostly UNCLAIMED data modules (lighthouse's third-party-web entities, emojilib, iconv-lite
tables, node-releases data, eslint-rule-docs, html entities). right -> wrong (1.7 KB in all) is cssstyle modules a
claim credited to @acemir/cssom (held-out, pop-bun: hand-checked, cssstyle's) and their test-bun / test2-bun
analogues (not inspected).

Verdict bytes over every judged library unit of the layers (`mapdefect14*.json`, factory and body spans both
counted): the label's closure holds the code for the bulk (held-out 336.7 of 442.9 MB), too little evidence for most
of the rest (93.6 MB), "ambiguous" (two families) held-out 98.6 KB, pop-bun 56.8, test2-bun 195.1; "nobody holds 80%"
at most 144 KB (test-rollup).

## 3. Adversarial audit (dev material only; excerpts private)

`scorer14/audit14.py`: pools from the ledger over the dev default labels (`exp135-prod`, `-apps`, `-bundlers`,
`pop-`, `remin-`), a seeded draw stratified by set, and per unit a PRIVATE dossier (in `/work/exp137-data`, never
committed): the unit's head and tail, its evidence tokens, the label's installed closure share, the other package's
best files with the context of the first token found, and every installed file holding half its evidence. Verdicts
(ids, spans, sizes, package names only): `data/audit.json`.

| sample                                                                                                                  | units | judged                                                                                                                                                                                                                                                                                                                                                    |   errors |
| ----------------------------------------------------------------------------------------------------------------------- | ----: | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------: |
| relabels, seeded draw (seed 137; heldout 14, pop-bun 13, apps-esbuild 6, apps-rollup 4, pop-esbuild 4, remin-esbuild 1) |    42 | the new package's file IS the unit's source (same strings, keys, messages, data rows) and the label's closure lacks it                                                                                                                                                                                                                                    |    **0** |
| relabels, the other 62 (census: every dev relabel)                                                                      |    62 | same                                                                                                                                                                                                                                                                                                                                                      |    **0** |
| left alone, any size (seed 137, 2 per set)                                                                              |    24 | 18 too little evidence (15 of them under 90 B: a bare name, a re-export; 3 of 405-756 B with 2-5 names), 3 label holds, 2 no install tree, 1 copy origin                                                                                                                                                                                                  | 0 misses |
| left alone, >= 1 KB (seed 1371)                                                                                         |    24 | 14 label holds (prettier's third-party.js carries js-yaml, mermaid carries cytoscape, stylelint vendors postcss-calc's parser ...), 3 label's file not at the map's path (the label's package holds it), 2 too little evidence, 2 copy origin, 2 no install tree, 1 ambiguous (a TLD list held by `tlds` and by `psl`'s bundles; the claim names neither) | 0 misses |

Error rate 0/104 (95% upper bound ~2.9% by the rule of three; 0/42 in the draw, bound ~7%) — under the 5% bar, so the
rule was not tightened. The 104 units are 42 distinct (label -> package) pairs, repeated across apps and unit shapes (css-tree modules
under lru-cache 21, node-releases data under baseline-browser-mapping 11, socks' constants under smart-buffer 9,
cssstyle modules under @acemir/cssom 7). The right -> wrong unit in
the draw (cssstyle's `CSSStyleDeclaration.js` requires, claimed as @acemir/cssom) is cssstyle's by its own code: the
claim was wrong.

## 4. Re-scored: main's defaults on the recommended index, scorer 13 vs 14

Labels `*exp135-prod*` (main's defaults incl. factory_bodies + the three cut-module keys, index
k2-all-r5-origin-prod, structural rule), both scopes; scorer 13 = the committed summaries at `origin/main`,
scorer 14 = this branch (`table14.py`, `data/table-main.json`). test (exp100) / test2 (exp116): summary numbers,
re-scored mechanically.

| set           | scorer 13 P / R / F0.5 | scorer 14 P / R / F0.5 |         dP |     dR |      dF0.5 | wrong KB 13 -> 14 |
| ------------- | ---------------------: | ---------------------: | ---------: | -----: | ---------: | ----------------: |
| cc            | 100.00 / 81.90 / 95.77 | 100.00 / 81.90 / 95.77 |     +0.000 | +0.000 |     +0.000 |        0.0 -> 0.0 |
| esbuild-app   |  99.07 / 74.74 / 93.01 |  99.07 / 74.74 / 93.01 |     +0.000 | +0.000 |     +0.000 |    131.1 -> 131.1 |
| heldout       |  99.56 / 68.51 / 91.29 |  99.64 / 68.56 / 91.35 | **+0.071** | +0.049 | **+0.065** |    704.2 -> 589.5 |
| wrangler      |  99.07 / 33.14 / 70.87 |  99.07 / 33.14 / 70.87 |     +0.000 | +0.000 |     +0.000 |        5.8 -> 5.8 |
| apps-esbuild  |  99.89 / 77.65 / 94.48 |  99.89 / 77.65 / 94.48 |     +0.003 | +0.002 |     +0.003 |    135.0 -> 131.8 |
| apps-rollup   |  99.09 / 79.31 / 94.38 |  99.09 / 79.31 / 94.38 |     +0.000 | +0.000 |     +0.000 |    710.6 -> 710.6 |
| apps-webpack  |  99.58 / 73.50 / 92.98 |  99.58 / 73.50 / 92.98 |     +0.000 | +0.000 |     +0.000 |    230.8 -> 230.8 |
| dev-rollup    |  99.87 / 77.53 / 94.43 |  99.87 / 77.53 / 94.43 |     +0.000 | +0.000 |     +0.000 |        2.1 -> 2.1 |
| dev-webpack   |  99.64 / 62.89 / 89.22 |  99.64 / 62.89 / 89.22 |     +0.000 | +0.000 |     +0.000 |        4.1 -> 4.1 |
| pop-bun       |  99.49 / 77.19 / 94.06 |  99.52 / 77.21 / 94.08 | **+0.023** | +0.018 | **+0.022** |  1277.1 -> 1218.5 |
| pop-esbuild   |  99.30 / 78.71 / 94.37 |  99.31 / 78.71 / 94.37 |     +0.002 | +0.001 |     +0.002 |  1799.4 -> 1795.2 |
| remin-esbuild |  99.69 / 51.76 / 84.11 |  99.69 / 51.76 / 84.11 |     +0.000 | +0.000 |     +0.000 |    160.0 -> 160.0 |
| remin-terser  |  99.90 / 20.33 / 56.04 |  99.90 / 20.33 / 56.04 |     +0.000 | +0.000 |     +0.000 |      18.6 -> 18.6 |
| test-bun      |  99.55 / 70.16 / 91.85 |  99.56 / 70.16 / 91.86 |     +0.006 | +0.004 |     +0.006 |    953.8 -> 940.7 |
| test-esbuild  |  99.26 / 73.37 / 92.72 |  99.26 / 73.37 / 92.72 |     +0.002 | +0.002 |     +0.002 |  1740.0 -> 1734.6 |
| test-rollup   |  98.96 / 72.43 / 92.21 |  98.96 / 72.43 / 92.21 |     +0.000 | +0.000 |     +0.000 |    829.8 -> 829.8 |
| test-webpack  |  99.48 / 72.19 / 92.49 |  99.48 / 72.19 / 92.49 |     +0.000 | +0.000 |     +0.000 |    568.4 -> 568.4 |
| test2-bun     |  99.26 / 69.37 / 91.38 |  99.36 / 69.43 / 91.48 | **+0.099** | +0.069 | **+0.091** |  1269.5 -> 1100.1 |
| test2-esbuild |  99.63 / 71.04 / 92.21 |  99.63 / 71.04 / 92.21 |     +0.002 | +0.002 |     +0.002 |    723.1 -> 718.9 |
| test2-rollup  |  96.35 / 75.65 / 91.35 |  96.44 / 75.73 / 91.44 | **+0.092** | +0.072 | **+0.087** |  1693.7 -> 1651.2 |
| test2-webpack |  99.27 / 70.09 / 91.64 |  99.28 / 70.09 / 91.65 |     +0.008 | +0.006 |     +0.007 |    874.1 -> 864.4 |

App claimed bytes 0 on every set under both. Precision never drops on any set; the Bun sets carry the move
(the defect is Bun's map); test2-rollup's +0.09 is 38.5 KB of claims (aggregate; not inspected).

**Every other committed label** (439 labels, 1,277 label-sets) was re-scored the same way: 915 label-sets identical,
348 moved with precision up or equal, 14 with precision down (two exp128 test-bun labels -0.018 points, twelve older (exp107-117)
apps-rollup labels -0.006 to -0.007). Cards and summaries are committed under
`096-library-matching-score/results/` as before (test / test2: summaries only).

## 5. Reproduce

```bash
S=experiments/137-scorer-mapdefect; PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
# the layer, per corpus (dev shown; apps/bundlers/pop/remin/test/test2 with their reference labels)
$PY -I $S/scorer14/mapdefect14.py build --corpus dev --units exp135-prod,exp130-origin --jobs 24
# rule off = scorer 13 (git diff on the results shows the schema string only)
$PY -I $S/verify_s13.py --corpus pop pop-exp135-prod
# ledger (both directions, coverage) and the audit pools; dev corpora only for --pool
$PY -I $S/scorer14/ledger14.py --corpus pop pop-exp135-prod --pool pool-pop.jsonl --json ledger-pop.json
$PY -I $S/scorer14/audit14.py pool-*.jsonl --relabel 42 --missed 24 --seed 137 --out <private dir>
$PY -I $S/scorer14/audit14.py pool-*.jsonl --relabel 0 --missed 24 --min-bytes 1000 --seed 1371 --out <private dir>
# every committed label (rescore_many per corpus; ~5-17 GB per process)
$PY -I experiments/120-truth-families/rescore_many.py --corpus C --cache <scratch> <labels...>
python3 -I $S/table14.py origin/main exp135-prod
```
