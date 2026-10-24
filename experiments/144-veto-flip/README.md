# exp144: a flip candidate for the isolation veto — `veto_support = "shown"`

## STATUS (2026-10-24)

DONE on branch `exp144-veto-flip` (off main `bdd468b5b`). **Default unchanged; nothing merged; no judge read.**
Dev corpora only (held-out by aggregate; test / test2 / remin2 never scored or diagnosed). Index: the recommended
`k2-all-r5-origin-prod-asof20261015` (id `c4d2bd6c`). Scorer 14, both scopes.

- **exp143's lever 1 reproduces on the new index.** `veto_support = "all-rules"` (config only, main binary): F0.5 up
  on 13 of 15 dev sets, the same two strict-bar failures as on the old index — apps-rollup P -0.040 (one 29 KB seed,
  `postcss-calc` claiming `stylelint`'s copy of its jison parser) and remin3-esbuild app claimed +49 B.
- **Mechanism of the 29 KB seed: a lone COPIED file whose shipper's version the index lacks.** `stylelint` 13.13.1
  ships `lib/utils/parseCalcExpression/parser.js`, a verbatim copy of `postcss-calc`'s parser. The index holds no
  `stylelint` 13.x (it holds 7.x and 14+), so the print is unambiguous for `postcss-calc`. `postcss-calc` itself is
  not in the bundle: its one seed is its only presence, and under `all-rules` the seed's own module fill (the
  wrapper's hoisted declarations, 2-34 B units) is its only support — self-support.
- **Generic fix, one new choice, no threshold: `[library.discovery] veto_support = "shown"`** (crates change,
  default off). A seed is supported as under the default (`without-module`), OR as under `all-rules` when BOTH
  (a) its family is SHOWN by the bundle without module fill — one of the family's index seeds survives the default
  veto, or it has a declared / banner seed — and (b) the index is not ambiguous about the seed (only one family's
  prints accept it; a fork's or a copy's seed gets no fill support). It never drops a seed the default keeps
  (`all-rules` alone can: its full fixpoint can hand the neighbour to another family; that cost 3.3 KB of right
  `rrweb-cssom` seeds on apps-rollup).
- **Result (`exp144-shown2`, commit `8592e0e9d`): P >= main on all 15 dev sets, F0.5 up on 13 (cc and dev-webpack
  unchanged), app claimed equal to main on 14.** held-out F0.5 91.385 -> 92.581 (+1.20, P 99.641 -> 99.656),
  esbuild-app +1.42, wrangler +1.24, apps-esbuild +0.64, pop +0.65 / +0.70, remin +1.39 / +1.10, remin3 +2.15 /
  +1.23, apps-rollup +0.02 (P +0.001). It keeps 80-99% of `all-rules`' F0.5 gain on held-out, esbuild-app, apps-esbuild, pop, remin-esbuild and
  remin3-esbuild, 44-47% on the two terser sets and 22% on apps-webpack (the single-module packages `shown` forgoes).
- **The one remaining strict-bar miss is a truth artifact: remin3-esbuild app claimed 26,998 -> 27,047 B (+49 B).**
  The unit is `Object.defineProperty(e,"__esModule",{value:!0});`, the first statement of a `__commonJS` module whose
  key in the bundle is `node_modules/graphql-tools/dist/generate/attachConnectorsToContext.js`; tsc's map has no
  segment for that preamble, so the truth labels it `<app>` while the same statement in every sibling
  `graphql-tools` module of the same bundle is labelled `graphql-tools`. The rescued seed (1,178 B, the module's
  body) is right. No pipeline rule should refuse it; a scorer rule (unmapped bytes inside a bundler module whose key
  names a package path belong to that package) would — not built here (it re-scores every label). **Andrew's call**
  whether this +49 B blocks the judge look.
- **Lever 2 (exempt an unambiguous seed holding >= half its module group) NOT built:** inside `shown` it would admit
  exp143's held-out index-defect unit (205 KB, `tapable`'s `lib/Hook.js` row holding `webpack`'s schema validator;
  sim held-out P -0.114) — the builder check below must come first. Outside `shown` it re-admits the `postcss-calc`
  seed (a whole-module unambiguous seed).
- **Builder check (plan only, no rebuild):** a unit row whose `moduleBytes` exceeds its installed file's size by
  a margin (Bun map defect: a row filed under a small file holding another package's large module) should be
  dropped or refiled at index build time.
- **Neutrality** (keys off, warm, vs `bdd468b5b`, 2.1.215 -> 2.1.216): see §5.

## 1. What was run

Labels (this branch's `score-all.sh`: every dev corpus, `--rule structural`, one label at a time):

| label stem      | commit      | `--pipeline-set`                           | what                                    |
| --------------- | ----------- | ------------------------------------------ | --------------------------------------- |
| `exp142-asof`   | `673c72cc0` | -                                          | main defaults on the new index (base)   |
| `exp144-vall`   | `673c72cc0` | `library.discovery.veto_support=all-rules` | exp143 lever 1                          |
| `exp144-shown`  | `2a2db1c27` | `library.discovery.veto_support=shown`     | `shown` without the ambiguity condition |
| `exp144-shown2` | `8592e0e9d` | `library.discovery.veto_support=shown`     | **the candidate**                       |

`673c72cc0` and `bdd468b5b` have identical `crates/` (harness-only commits between), so `exp142-asof` IS main's
defaults on this index (exp142's labels, scorer 14, reused).

## 2. Diagnosis (dev data only)

`scripts/newwrong.py` lists every unit whose verdict changed between two labels (rows name bundles: kept in
`/work/exp144-data`, not committed). On the new index `all-rules` adds, vs main:

| set            | gained right KB | new wrong KB | lost right KB |
| -------------- | --------------: | -----------: | ------------: |
| heldout        |           8,682 |         20.1 |           5.2 |
| esbuild-app    |             872 |          0.0 |           0.2 |
| apps-esbuild   |           3,632 |          1.2 |           7.3 |
| apps-rollup    |             101 |     **31.9** |           5.1 |
| apps-webpack   |             449 |          0.0 |           0.0 |
| pop-bun        |           8,820 |         24.2 |          15.7 |
| pop-esbuild    |           9,564 |         12.7 |          16.6 |
| remin-esbuild  |           3,031 |          0.3 |           5.6 |
| remin-terser   |           1,480 |          0.0 |          14.8 |
| remin3-esbuild |           9,240 |         29.9 |           4.3 |
| remin3-terser  |           3,252 |          0.3 |          19.3 |

The new wrong is copies and forks: apps-rollup `postcss-calc` <- `stylelint` 29.0 KB (§STATUS), `react-autowhatever`
<- `react-soundplayer` 1.6, `@achrinza/node-ipc` <- `node-ipc` 0.7, `source-map` <- `source-map-js` 0.3; remin3-esbuild
`yargs` <- `@curlconverter/yargs` 23.9 KB (exp141's fork); pop-bun `css-tree` / `stylelint` <- `lru-cache`.

**Simulation before building** (`scripts/simpresent.py` + `scripts/simbar.py`; claims of a package with no kept
seed in the base run dropped; no propagation modelled): the presence condition alone removes 29.1 of apps-rollup's
31.9 KB and 27.9 of remin3-esbuild's 29.9 KB new wrong, and gives up 0-2.5 MB of right per set (icon packs and
single-module dependencies whose one file is all the bundle holds: `@ant-design/icons-svg` 720 KB, `@grpc/grpc-js`
343 KB). Sim P >= main on 14 of 15; apps-rollup stays 0.002 short (99.110 vs 99.112: the remaining 2.8 KB are
copies / forks of SHOWN families). Floors on "shown" (>= 2 kept seeds, >= 1 / 2 / 4 KB of them) did not close it
(sim: apps-rollup FAIL under every floor) — the residual is ambiguity, not presence: exp143's seed-level sim
(`vetoed-seeds.json`, old index) already measured that unambiguous vetoed seeds carry 2-30% less right but 3-4x less
wrong (apps-rollup 550 / 81 -> 387 / 22 KB right / wrong; pop-bun 7,886 / 277 -> 7,838 / 83). Measured: the
ambiguity condition (shown -> shown2) removes apps-rollup's remaining wrong (693.1 -> 690.3 KB; main 690.2).

Lost right under `all-rules` that `shown` does not recover: `all-rules`' fixpoint also changes which seeds the
targeted (queue) round produces; e.g. apps-rollup's `rrweb-cssom` seeds are vetoed under both relaxed rules because
a rescued sibling seed changes the round's claims. 5.1 KB on apps-rollup; not pursued.

The +49 B app claim on remin3-esbuild: §STATUS. It survives every variant (the seed is unambiguous and its family
shown) and is a truth gap, not a wrong claim.

No threshold was chosen in this lane, so there is no split-half or sensitivity table: `shown` has none.

## 3. Results (scorer 14, both scopes; P / R / F0.5 in points, wrong = wrongFamily + selfOther KB, app = app-claimed bytes; **x** = fails the bar vs main: P >= main, F0.5 >= main - 0.002, app <= main)

| set            | main (`exp142-asof`)                   | `all-rules` (`exp144-vall`)                  | `shown` w/o ambiguity (`exp144-shown`)       | **`shown` (`exp144-shown2`)**                                  |
| -------------- | -------------------------------------- | -------------------------------------------- | -------------------------------------------- | -------------------------------------------------------------- |
| cc             | 100.000 / 81.90 / 95.768, 0.0, 0       | 100.000 / 81.90 / 95.768, 0.0, 0             | 100.000 / 81.90 / 95.768, 0.0, 0             | 100.000 / 81.90 / 95.768, 0.0, 0                               |
| esbuild-app    | 99.115 / 75.60 / 93.310, 122.5, 0      | 99.168 / 80.40 / 94.744, 122.5, 0            | 99.167 / 80.35 / 94.731, 122.5, 0            | 99.167 / 80.35 / 94.731, 122.5, 0                              |
| heldout        | 99.641 / 68.64 / 91.385, 648.4, 0      | 99.647 / 72.42 / 92.677, 668.3, 0            | 99.651 / 72.11 / 92.580, 660.0, 0            | 99.656 / 72.11 / 92.581, 652.4, 0                              |
| wrangler       | 99.069 / 33.14 / 70.870, 5.6, 0        | 99.106 / 34.51 / 72.109, 5.6, 0              | 99.106 / 34.51 / 72.109, 5.6, 0              | 99.106 / 34.51 / 72.109, 5.6, 0                                |
| apps-esbuild   | 99.908 / 77.65 / 94.491, 113.8, 0      | 99.909 / 80.08 / 95.194, 115.0, 0            | 99.909 / 79.85 / 95.129, 114.9, 0            | 99.909 / 79.85 / 95.129, 114.9, 0                              |
| apps-rollup    | 99.112 / 79.31 / 94.398, 690.2, 0      | 99.072 / 79.41 / 94.398, 722.0, 0 **x**      | 99.110 / 79.40 / 94.422, 693.1, 0 **x**      | 99.113 / 79.38 / 94.419, 690.3, 0                              |
| apps-webpack   | 99.599 / 73.51 / 92.997, 215.5, 0      | 99.602 / 74.12 / 93.195, 215.5, 0            | 99.599 / 73.65 / 93.041, 215.5, 0            | 99.599 / 73.64 / 93.041, 215.5, 0                              |
| dev-rollup     | 99.873 / 77.53 / 94.430, 2.1, 0        | 99.873 / 77.74 / 94.493, 2.1, 0              | 99.873 / 77.74 / 94.493, 2.1, 0              | 99.873 / 77.67 / 94.471, 2.1, 0                                |
| dev-webpack    | 99.642 / 62.89 / 89.216, 4.0, 0        | 99.642 / 62.89 / 89.216, 4.0, 0              | 99.642 / 62.89 / 89.216, 4.0, 0              | 99.642 / 62.89 / 89.216, 4.0, 0                                |
| pop-bun        | 99.512 / 77.81 / 94.255, 1,234.3, 0    | 99.520 / 80.58 / 95.051, 1,257.3, 0          | 99.519 / 80.06 / 94.907, 1,252.2, 0          | 99.522 / 80.06 / 94.908, 1,244.2, 0                            |
| pop-esbuild    | 99.311 / 79.33 / 94.549, 1,830.1, 0    | 99.331 / 82.33 / 95.391, 1,842.2, 0          | 99.327 / 81.81 / 95.248, 1,842.1, 0          | 99.328 / 81.81 / 95.249, 1,839.3, 0                            |
| remin-esbuild  | 99.720 / 51.89 / 84.197, 142.5, 0      | 99.735 / 55.00 / 85.781, 142.7, 0            | 99.733 / 54.61 / 85.587, 142.7, 0            | 99.733 / 54.60 / 85.583, 142.7, 0                              |
| remin-terser   | 99.916 / 20.43 / 56.191, 15.6, 0       | 99.922 / 22.04 / 58.541, 15.6, 0             | 99.919 / 21.17 / 57.294, 15.6, 0             | 99.919 / 21.17 / 57.294, 15.6, 0                               |
| remin3-esbuild | 99.679 / 49.71 / 82.994, 297.1, 26,998 | 99.688 / 54.28 / 85.401, 316.7, 27,047 **x** | 99.711 / 53.78 / 85.164, 288.8, 27,047 **x** | 99.702 / 53.75 / 85.143, 298.5, 27,047 **x** (+49 B truth gap) |
| remin3-terser  | 99.849 / 17.58 / 51.585, 51.0, 116     | 99.861 / 19.27 / 54.381, 51.3, 116           | 99.855 / 18.31 / 52.814, 51.0, 116           | 99.855 / 18.31 / 52.814, 51.0, 116                             |

Full rows: `data/table.json` (`scripts/table.py`). shown -> shown2 on remin3-esbuild: wrong 288.8 -> 298.5 KB is one
unit of one bundle (10.3 KB, `dompurify` -> `mermaid`: an ambiguous `dompurify` seed loses its fill support and
`mermaid`, which ships `dompurify`, takes the unit by propagation; main has the same wrong).

## 4. The judge look (NOT run here)

The candidate is `--pipeline-set library.discovery.veto_support=shown` at commit `8592e0e9d`:

```bash
npm run eval -- library test-exp144-shown2 --index /work/exp142-data/full/ix/k2-all-r5-origin-prod-asof20261015 --rule structural --corpus test --commit 8592e0e9d --pipeline-set library.discovery.veto_support=shown
npm run eval -- library test2-exp144-shown2 --index /work/exp142-data/full/ix/k2-all-r5-origin-prod-asof20261015 --rule structural --corpus test2 --commit 8592e0e9d --pipeline-set library.discovery.veto_support=shown
npm run eval -- library remin2-exp144-shown2 --index /work/exp142-data/full/ix/k2-all-r5-origin-prod-asof20261015 --rule structural --corpus remin2 --commit 8592e0e9d --pipeline-set library.discovery.veto_support=shown
```

Bases: `test-exp142-asof`, `test2-exp142-asof`, `remin2-exp142-asof` (main defaults on this index, exp142).

## 5. Gate and neutrality

| check                                                                                  | result                                                                    |
| -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| red -> green: `shown_support_*` (3 discovery tests), `the_exp144_shown_veto_support_*` | red seen for each (assertion failures with the condition off), then green |
| warm neutrality, default, `8592e0e9d` vs `bdd468b5b`, 2.1.215 -> 2.1.216               | NEUTRALITY_DEFAULT                                                        |
| warm neutrality, report mode (`library.matching=report`, new index), same pair         | NEUTRALITY_REPORT                                                         |
| `npm run check` (bare, `/work/exp144-data/gate.log`)                                   | GATE_RESULT                                                               |

## 6. Files

- `scripts/score-all.sh` (exp143's, relabelled), `newwrong.py` (changed-verdict rows), `simpresent.py` (presence /
  floor sim), `simbar.py` (sim -> P / F0.5 / bar), `absentgroups.py`, `flanks.py` (the layout signal, refuted: a
  copy's flanks look like a dependency's), `table.py`.
- Crates: `crates/humanify-core/src/libdetect/discovery.rs` (`VetoSupport::Shown`, `unshown`), tests in
  `discovery_test.rs`; config choice in `crates/humanify-cli/src/config{,/keys}.rs`; docs in
  `docs/library-matching-config.md`, `docs/design/library-discovery.md` §7 guard 4, `humanify.example.toml`.
- Labels `exp144-{vall,shown,shown2}` (+ `-apps`, `-bundlers`, `pop-`, `remin-`, `remin3-`): cards and summaries
  under `experiments/096-library-matching-score/results/`.
