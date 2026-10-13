# exp097: index recipes — the library index built around the packing tool

## STATUS (2026-10-13)

- **State.** DONE. Branch `exp097-index-recipes` (merged with main through `072ed5c3`), not merged, not pushed.
  Four changes, each its own commit and its own scored label pair (structural + agreement rule) on the exp096
  library score. Library matching stays OFF by default; the pipeline's output did not move (warm neutrality,
  below).
- **Totals first** (recall = share of library statement bytes named with the right package or family; no label
  claims ANY app code — the hard bar holds on every set, every step):

  | label (rule)          | Claude Code 2.1.216 | held-out apps (67) | task-master-ai (esbuild) | wrangler (esbuild, re-minified) |
  | --------------------- | ------------------- | ------------------ | ------------------------ | ------------------------------- |
  | integ2 (structural)   | 37.1% / 0.0%        | 20.4% / 4.9%       | 33.7% / 8.3%             | 7.5% / 6.0%                     |
  | step 1 units/3        | 37.1% / 0.0%        | 20.4% / 4.9%       | **37.1%** / 8.1%         | 7.6% / 5.9%                     |
  | step 2 recipe slices  | **53.8%** / 0.0%    | 20.4% / 4.9%       | 37.1% / 8.1%             | 7.6% / 5.9%                     |
  | step 3b sponge filter | 53.8% / 0.0%        | 20.4% / 4.9%       | 37.1% / 8.1%             | 7.6% / 5.9%                     |
  | step 4 CC pins        | **67.1%** / 0.0%    | 20.7% / 5.1%       | 38.5% / 8.2%             | 7.6% / 5.9%                     |
  | integ2 (agreement)    | 23.0% / 0.0%        | 12.6% / 7.0%       | 20.2% / 3.8%             | 6.6% / 0.0%                     |
  | step 1 units/3        | 23.0% / 0.0%        | 12.6% / 7.0%       | **21.6%** / 3.6%         | 6.6% / 0.0%                     |
  | step 2 recipe slices  | **29.4%** / 0.0%    | 12.6% / 7.1%       | 21.6% / 3.6%             | 6.6% / 0.0%                     |
  | step 3b sponge filter | 29.4% / 0.0%        | 12.6% / 7.1%       | 21.6% / 3.6%             | 6.6% / 0.0%                     |
  | step 4 CC pins        | **41.9%** / 0.0%    | 12.9% / 7.2%       | 22.8% / 4.0%             | 6.6% / 0.0%                     |

  Each cell is recall / wrong-family share. The score is deterministic (band 0), so every move is real. The full
  leaderboard is `npm run eval -- library-leaderboard integ2-structural exp097-s1-structural …` over the labels in
  `../096-library-matching-score/results/exp097-*`.

- **Step 1 (units/3), prediction vs result.** Predicted (exp096): task-master-ai structural recall +3.4 to +4.4
  points, wrong family flat, wrangler up. Result: **+3.4** (33.7% → 37.1%), at the low end; agreement +1.4. Its
  "hash differs" miss fell 3.7% → 1.1%. Wrong family went slightly DOWN (8.3% → 8.1%; agreement 3.8% → 3.6%).
  **Wrangler did not move** (+0.1): its misses are versions the index does not hold (42.9%) and packages it does
  not hold (32.2%), not the stale lazy wrappers. Bun inputs did not move, by construction: the rebuilt Bun unit
  table is byte-identical to index v2's.
- **Step 2 (recipe slices).** Claude Code (Bun `--target bun`, read off its `// @bun @bytecode @bun-cjs` banner)
  now reads the new Bun target-bun slice: structural 37.1% → 53.8%, its "hash differs" miss 17.4% → 1.6%.
  Under agreement it gains less (23.0% → 29.4%): the literal rule now refuses most of the new matches (guard 12.7%
  → 20.5%). Every other input reads the slice it read before (held-out: Bun target node, no banner; esbuild: the
  index's esbuild slice) and is unchanged.
- **Step 3 (sponge filter): first rule REFUTED, refined rule inert.** The first rule (file name, any comment
  naming a `node_modules/` path, vendored directory) LOST held-out recall (20.4% → 19.6%) and RAISED wrong family
  (4.9% → 5.1%): it dropped esquery's own `dist/esquery.min.js` (its main) and glob files whose comments mention
  node_modules. Step 3b keeps only a vendored directory or a strict bundler module marker: it drops ~0.3% of unit
  rows (about 80,000 per slice, 1,264–1,748 vendored files and 130–138 marked files) and moves NOTHING scored
  beyond one shifted error: prettier 2's pre-bundled `index.js` is gone, and the same @google/genai bytes are now
  named `web-streams-polyfill` instead. **The measured wrong names are not sponge files**: they are dependency code
  compiled into a consumer with no marker (d3-delaunay's rollup UMD, boxen, got) and shared TypeScript-emitted
  helpers (the genai bytes). Recall did not fall, so the gate's condition holds, but nothing was gained.
- **Step 4 (Claude Code pins from its own declarations).** The bundle declares three versions:
  `@anthropic-ai/sdk@0.94.0` (the Stainless version constant behind `X-Stainless-Package-Version` and the
  `Anthropic/JS` user agent) and `@azure/msal-node@3.8.1` / `@azure/msal-common@15.13.1` (kept license banners).
  **MCP SDK, growthbook and ink declare no version anywhere in the bundle**; they were added by name under the
  index's normal 12-month policy (50 / 48 / 45 versions), so their part of the gain is a package-choice fit, not
  a declared pin. `scripts/cc_declared.py` reproduces the declarations (names, versions, byte offsets only).
  Claude Code, "pinned from its own declarations" plus those three names: structural 53.8% → **67.1%**, agreement
  29.4% → **41.9%**, "not indexed" 33.8% → 17.2%, 0 app, 0 wrong family. The held-out apps gain a little (+0.3)
  because some ship the same packages; their wrong family moves 4.9% → 5.1%.

- **What still blocks the ~0.2% wrong-name bar** (held-out 4.9–7.2%, task-master-ai 3.6–8.2%; Claude Code and
  agreement-wrangler are already at 0): (1) dependency code compiled into a consumer with no marker — needs the
  copied package's own build to win the tie, i.e. a containment rule with a direction (probed in step 3 and NOT
  adopted: a partial vendored copy looks like its origin — `jsbn` ⊃ `ssh2-streams/lib/jsbn.js`); (2) helpers every
  TypeScript build emits, claimed by whichever package holds them; (3) the remaining vendored copies the truth
  still credits to the shipper (exp096: 5 of 49).

## Gate and neutrality

| check                                                                                                                          | result                                                          |
| ------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| `npm run check` (13 stages) on every commit: steps 1, 2, 3, 3-fix, 3b, and each merge of main                                  | ALL 13 STAGES PASSED each time                                  |
| warm neutrality, `e6a81298` (main at the time) vs this branch at step 3, 2.1.215→2.1.216                                       | **NEUTRAL**: 0 files, 0 lines, cache +0 / +0, exit 0 / 0        |
| final warm neutrality, `072ed5c3` (main) vs `6becc20b` (this branch, all steps), `npm run eval -- neutrality`, 2.1.215→2.1.216 | **NEUTRAL**: 0 files, 0 lines, cache +0 / +0, exit 0 / 0        |
| the merged binary (main's matching step 3) re-scoring the step-2 index (`exp097-s2m-structural`)                               | every card identical to `exp097-s2-structural` except its label |

The neutrality baseline is main, not `5cd68c29`: main moved three times during the lane (each move proven neutral
by its own lane), and main is where this branch's own change is isolated.

## 1. What changed (code)

- **Step 1 — the unit model's version, and one owner for "can this binary read this index?"**
  `hash::forms::LIBRARY_UNITS` 2 → 3. Main's 68b090e3 made esbuild 0.28's guarded `__esm` a lazy module, so its
  init body became lazy-body units, but the stamp stayed `units/2` and the builder reused every stale esbuild
  shard. A new guard pins the digest of every unit print of six e2e fixtures per version
  (`libindex/units_test.rs` `UNIT_MODEL_PINS`): index v2's builder (a537a92b) reads `5b0dc0e8`, today `3e6f96ad`;
  only the two esbuild-0.28 minified fixtures differ. `libindex::format::Manifest::check_compatible` compares
  every stamp — format, module key, literals, units, each slice's recipe version — and the CLI's parse-point
  refusal (`config::library_index_stale`) delegates to it.
- **Step 2 — recipe slices.** `libindex::recipe`: a recipe is bundler + target + format + minify, spelled
  `bun:target=bun,format=cjs,minify` (every key named, no defaults), slice name `bun-bun-cjs-min`. The bundle's
  recipe is read once (`detect_recipe`: Bun's banner says `--target bun` and `@bun-cjs`; no banner = `--target
node`; esbuild records no platform, so its bundles take the index's default esbuild slice), and the slice is
  chosen in one place (`choose_slice`, on bundler + target). Index format `humanify-library-index/2`; an older
  index is refused by its stamps before the strict parse. No slice for the recipe: `--library-matching report`
  prints `WARNING: library index <id> has no slice for recipe bun:target=node,… (has: …)` and records it as
  `recipeMissing`; `humanify discover` fails. Builder: `--recipe` (required, repeatable) replaces `--slices`.
- **Step 3 / 3b — sponge evidence for unit rows** (`humanify-index` `sponge_evidence`): a vendored directory, or
  a module-boundary comment that names another package's `node_modules` path (esbuild/Bun `// <path>`, rolldown
  `//#region <path>`, webpack `/***/ "<path>":`, `/*!*** <path> ***!*/`). Counted per evidence in each shard's
  notes. Module shards carry `unit-rows/4`.
- **Step 4** has no code: the pins are data (`/work/exp097-data/cc-declared-pins.txt`, read by
  `scripts/cc_declared.py`).

## 2. Index builds (24 cores, `taskset 40-63`; incremental over one shard store)

| index                        | id         | size   | shards built / reused | wall                           | what ran                                                                                              |
| ---------------------------- | ---------- | ------ | --------------------- | ------------------------------ | ----------------------------------------------------------------------------------------------------- |
| v2 (exp095, reference)       | `b00fa735` | 440 MB | —                     | ~60 min                        | —                                                                                                     |
| step 1 (units/3)             | `b95dc9ed` | 440 MB | 46,221 / 20,847       | 35.6 min (28 shards + 8 merge) | re-fingerprint of both slices; no bundler ran                                                         |
| step 2 (3 recipe slices)     | `e3c0ee92` | 566 MB | 43,082 / 46,342       | 37.2 min (28.5 + 8.7)          | Bun `--target bun` builds of every version; the other two slices re-fingerprinted from adopted builds |
| step 3 (first sponge rule)   | `8c16f5f4` | 557 MB | 68,577 / 20,847       | 45.8 min (37 + 8.6)            | re-fingerprint, all three slices                                                                      |
| step 3b (refined)            | `c7f20f88` | 564 MB | 68,577 / 20,847       | 45.3 min (36.6 + 8.8)          | re-fingerprint, all three slices                                                                      |
| step 4 (+ CC pins and names) | `4d1db975` | 577 MB | 6,552 / 83,456        | 12.8 min (4.1 + 8.7)           | 146 new versions (and the 1,509 versions npm cannot install, retried)                                 |

Per slice (step 2): Bun target bun units 100.4 MB + modules 10.4 MB; Bun target node 96.9 + 12.0; esbuild
95.7 + 12.6; literals 224.6 MB (was 212.8: the target-bun builds pull in `module`-field files); manifest 12.8.
The Bun target-bun slice costs about +126 MB (+29%); its Bun builds ran inside step 2's 28.5-minute shard phase (with the re-fingerprint of the other two slices).

`scripts/adopt_builds.py` hard-linked index v2's 82,962 stamped builds under their recipe slice names (same
bundler binaries, same command lines), so step 2 ran Bun only for the new target.

## 3. Things found on the way

- **A build's sourcemap paths are relative to the out directory AS THE BUNDLER SAW IT.** v2's builds were made
  through `/work/exp095-index-v2-data/final/work` (a symlink); a first step-1 build through another symlink of
  the same tree attributed NO unit (every unit "unowned") and merged an index with empty unit tables, silently.
  `build-index.sh` now takes the work path verbatim. The builder should refuse a build whose units are all
  unowned (follow-up).
- **The shard store path is not keyed by the shard key**: a newer key overwrites the older shard at the same
  path, so two builds with different versions cannot share a store concurrently. Each step's index was merged
  before the next step's build started.
- The raw Claude Code bundle has lone-surrogate escapes that the unit walks' JSON tree cannot hold (coordinator,
  2026-10-13); the exp096 instrument feeds the raw bundle, so Claude Code units with such strings may be missing
  from both recall's numerator and denominator. Not measured here.

## 4. How to reproduce

```bash
D=/work/exp097-data; W=/work/exp095-index-v2-data/final/work
experiments/097-index-recipes/scripts/build-index.sh $D $W step2 40-63 <humanify-index> $D/base.txt $D/pins.txt -- \
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify \
  --recipe esbuild:target=node,format=cjs,minify --bun <bun 1.4.0> --esbuild <esbuild 0.28.2>
npm run eval -- library exp097-s2-structural --index $D/ix/step2 --rule structural --commit <sha> --cpus 16-31
python3 -I experiments/097-index-recipes/scripts/cc_declared.py <claude code bundle>      # step 4's pins
```

Labels → commits: s1 `f1a4f9fd`, s2 `c377d8f5`, s3 `c65b199a` (step 3 + main's matching step 3), s3b and s4
`7a593970`. Bulk (indexes, store, logs) is under `/work/exp097-data`, not committed.
