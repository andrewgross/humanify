# exp130: crediting foreign regions to their origin (`--sponge-regions origin`), re-judged under scorer 10

## STATUS (2026-10-17)

DONE on branch `exp130-sponge-origin` (off main `ca6353ee`; main had not moved at the final scoring, so the
scorer is main's scorer 10 and the defaults are main's). Builder-only change (`crates/humanify-index`, default
unchanged: `drop`), so no neutrality run is needed. The bare gate passed all 13 stages before each commit.

- **`origin` HOLDS under scorer 10.** k2-all re-fingerprinted with `--sponge-regions origin`
  (`/work/exp130-data/full/ix/k2-all-r5-origin`, 6.5 GB, index id `c5fe9a74…`) vs the shipped k2-all-r5, main's
  defaults, all 7 corpora (22 sets): F0.5 up on 16, equal on 5, down on 1; precision up or equal on 21; app
  claimed 0 everywhere. pop-bun +4.11, pop-esbuild +4.26, test-bun +3.09, test-esbuild +3.10, wrangler +3.84,
  test-rollup +1.65, test-webpack +0.68, apps-esbuild +0.44 (P 99.29 -> 99.77), apps-rollup +0.24, held-out
  +0.08, remin-terser +0.08.
- **The one loss: test2-esbuild -0.03** (P 99.49 -> 99.46, R 75.80 -> 75.76; +8 KB wrong, -17 KB named of
  43 MB). Its dev analogue is in every dev corpus at the same scale (§4): new wrong claims that are mostly TRUTH
  gaps (pnpm patched dependencies, `_patch_hash=` marker paths, which the truth's marker regex refuses;
  marker-less inlined code) or a granularity effect (a Bun lazy-init statement that is 45% an inlined
  dependency), plus copies the carrier used to name that now split owners across two families and go unclaimed.
  apps-webpack (+0.01, +1.6 KB wrong) is the nearest dev twin. Tiny and explained -> **recommended as the new
  default index** (`docs/design/library-index.md` §10).
- **The rebuild is exact.** The lab (k2-all-r5's store with only the 8,166 bundler-marker shards rebuilt) scores
  identically to exp124's deleted `*exp124-orig*` labels on all 19 shared sets, and the full re-fingerprint
  (every module shard, real `unit-rows/5+origin` keys) wrote every index table byte-identical to the lab's; only
  `manifest.json` (the keys and id) differs. Its scores equal the lab's on all 22 sets.
- **Declared twin difference** (`docs/responsibility.md`): the builder's marker rule takes any path, the truth's
  `PATHLIKE` refuses `=`. A scorer fix (accept pnpm `_patch_hash=` paths) would turn some of §4's new wrong into
  right; that belongs with the copies/truth lanes (exp129), not here.

## 1. The question

exp124 made the index builder region-aware (`unit-rows/5`, the shipped k2-all-r5): a module marker in a package's
own pre-bundled file drops only the units inside the FOREIGN region it opens. It also measured the alternative,
`origin`: credit a foreign region's units to the package its marker names, at the version its pnpm store path
spells (no version: dropped). Under scorer 9 that lost test-bun precision 97.79 -> 92.59, so the flag was deleted.

exp125 (scorer 10, rule `via`) then showed that the precision loss was a truth gap: a nested chunk's publisher. On
the summaries, the old `*exp124-orig*` labels beat k2-all-r5 on 14 of 19 sets, tied 4 and lost 1 (test2-esbuild
-0.03). This lane re-judges `origin` honestly: rebuild it (the exp124 lab index was deleted), score it on every
corpus at main's current scorer and defaults, explain the one loss on dev material, and if it holds, re-fingerprint
k2-all with it to a new path.

## 2. The builder change

`crates/humanify-index/src/build.rs`: `SpongeRegions` (`Drop`, the default, and `Origin`) and `Origin` (package,
pnpm version, file) are back, with `marker_origin` and the per-origin vote in `Sponge::unit_owner` (a unit takes
the origin that holds at least `PURITY` = 90% of its segments in the file). `build --sponge-regions drop|origin`
selects the mode. Origin shards are keyed `unit-rows/5+origin`, so a store can never mix the two. The code is
exp124's (`b3bea3017`), restored test-first: the test
`a_foreign_region_can_be_credited_to_the_package_its_marker_names` (and the mode argument in the region tests) went
in first and failed to compile (red), then the code (green).

`docs/responsibility.md` (the builder unit-rows row) now declares one difference between the builder's marker rule
and its truth twin, found while explaining the loss (section 4): the builder takes any marker path, the truth's
`PATHLIKE` refuses a `=`, so a pnpm PATCHED dependency's region (`.pnpm/sirv@3.0.2_patch_hash=…/`) is the shipper's
own code to the truth.

## 3. Lab first: the rebuilt origin index reproduces exp124's labels exactly

The lab followed exp124's protocol, on top of k2-all-r5 instead of k2-all:

- store: every k2-all-r5 shard hard-linked, minus the 8,166 module shards whose notes record a bundler-marker drop
  (1,822 versions; the only shards `origin` can change, since it only re-owns units `drop` dropped);
- work dir: `/work/exp130-data/work`, every install dir hard-link-copied from `/work/exp124-data/work`, the SAME
  work dir r5's builds were made in, at the same depth (so relative source paths resolve identically);
- binary: this branch with `Origin` keyed `unit-rows/5` (lab only, so the unaffected shards are reused);
- `flock /work/index-build.lock`, cores 34-63: 58 min of shards, 20 min merge (34.6 GB peak), 6.5 GB, index id
  `7e4a4ac0…` (`/work/exp130-data/lab/ix/k2-all-r5-origin-lab`).

Scored on all 7 corpora at this branch (`0a698c64`, main `ca6353ee` + the builder flag; main's shipped defaults,
structural, scorer 10). `exp130-r5` is a fresh control on k2-all-r5 at the same commit. It equals `exp124-r5` on
every set, and `exp130-olab` equals exp124's `*exp124-orig*` on every one of the 19 sets they share, so the old
labels' numbers stand and the rebuild is the same index in effect.

P / R / **F0.5**, delta F0.5 vs k2-all-r5:

| set           | k2-all-r5 (`exp130-r5`)    | origin (`exp130-olab` = `exp130-origin`) |
| ------------- | -------------------------- | ---------------------------------------- |
| cc            | 100.00 / 81.90 / **95.77** | 100.00 / 81.90 / **95.77** (+0.00)       |
| esbuild-app   | 96.60 / 69.44 / **89.59**  | 96.61 / 69.60 / **89.65** (+0.06)        |
| heldout       | 99.31 / 67.60 / **90.79**  | 99.31 / 67.82 / **90.87** (+0.08)        |
| wrangler      | 95.99 / 18.53 / **52.28**  | 96.45 / 21.00 / **56.12** (+3.84)        |
| apps-esbuild  | 99.29 / 92.15 / **97.78**  | 99.77 / 92.44 / **98.21** (+0.44)        |
| apps-rollup   | 98.98 / 78.66 / **94.12**  | 99.06 / 79.28 / **94.35** (+0.24)        |
| apps-webpack  | 99.39 / 73.30 / **92.79**  | 99.39 / 73.36 / **92.80** (+0.01)        |
| dev-rollup    | 99.87 / 77.53 / **94.43**  | 99.87 / 77.53 / **94.43** (+0.00)        |
| dev-webpack   | 99.64 / 62.89 / **89.22**  | 99.64 / 62.89 / **89.22** (+0.00)        |
| pop-bun       | 95.51 / 79.34 / **91.77**  | 98.79 / 85.79 / **95.88** (+4.11)        |
| pop-esbuild   | 94.08 / 81.42 / **91.24**  | 97.58 / 88.01 / **95.50** (+4.26)        |
| test-bun      | 97.85 / 67.56 / **89.80**  | 99.31 / 73.79 / **92.89** (+3.09)        |
| test-esbuild  | 97.00 / 76.80 / **92.16**  | 99.00 / 82.73 / **95.25** (+3.10)        |
| test-rollup   | 97.94 / 68.80 / **90.29**  | 98.68 / 72.23 / **91.94** (+1.65)        |
| test-webpack  | 99.24 / 70.62 / **91.80**  | 99.48 / 72.19 / **92.49** (+0.68)        |
| test2-bun     | 98.85 / 79.30 / **94.21**  | 98.85 / 79.55 / **94.28** (+0.07)        |
| test2-esbuild | 99.49 / 75.80 / **93.64**  | 99.46 / 75.76 / **93.61** (-0.03)        |
| test2-rollup  | 96.33 / 75.35 / **91.25**  | 96.34 / 75.65 / **91.35** (+0.10)        |
| test2-webpack | 99.27 / 70.08 / **91.64**  | 99.27 / 70.09 / **91.64** (+0.00)        |
| remin-esbuild | 99.04 / 78.17 / **94.02**  | 99.04 / 78.17 / **94.02** (+0.00)        |
| remin-terser  | 99.82 / 60.45 / **88.32**  | 99.82 / 60.64 / **88.40** (+0.08)        |

App claimed is 0 in every cell. test and test2 are summaries only; test2 was never diagnosed.

- F0.5 up on 16 of 22 sets, equal on 5 (cc, dev-rollup, dev-webpack, test2-webpack, remin-esbuild), down on 1
  (test2-esbuild, -0.03).
- Precision up or equal on 21 of 22. test2-esbuild: 99.49 -> 99.46.
- Biggest moves: pop +4.11 / +4.26, test-bun / test-esbuild +3.09 / +3.10, wrangler +3.84, test-rollup +1.65.

## 4. The one loss, test2-esbuild -0.03, explained on dev material

test2-esbuild's summary moves +8,037 B wrong-family and -17,134 B named (of 43.1 MB library). Every dev corpus
shows the same two counter-currents at the same small scale; on the sets where origin wins, a much larger gain
outweighs them. test2-esbuild simply has no large gain to cover them.

The diff (`scripts/jdiff.py`: every claim of `exp124-r5` and `exp124-orig` judged as score.py judges it, dev
corpora only; it reproduces each summary's wrong-family delta to the byte):

| set          | new wrong, truth = shipper's own (A) | new wrong, truth names another origin (B) | wrong fixed | good -> unclaimed |
| ------------ | -----------------------------------: | ----------------------------------------: | ----------: | ----------------: |
| held-out     |                                  859 |                                       917 |       1,658 |            55,821 |
| apps-esbuild |                                3,016 |                                     1,001 |     118,522 |            28,774 |
| apps-rollup  |                               12,884 |                                     3,326 |      71,506 |            27,226 |
| apps-webpack |                                    0 |                                     1,732 |          97 |            36,332 |
| pop-bun      |                               19,191 |                                     7,035 |   2,735,615 |            90,064 |
| pop-esbuild  |                               19,117 |                                    17,391 |   3,018,847 |            66,368 |

(bytes; "fixed" = wrong under r5, good or unclaimed under origin.) apps-webpack is the closest dev analogue of
test2-esbuild: wrong +1,635 B net and recall gained elsewhere only just covering it (+0.01).

**A. New wrong where the truth says the shipper's own code (hand-checked, the top units):**

- **pnpm patched dependencies, a truth gap.** vite 8.3.4's `dist/node/chunks/node.js` inlines chokidar under
  `//#region ../../node_modules/.pnpm/chokidar@3.6.0_patch_hash=e172…/node_modules/chokidar/…`, and vite 7.3.6's
  `config.js` inlines sirv under `.pnpm/sirv@3.0.2_patch_hash=c07c…/`. The truth's marker regex (`PATHLIKE`) refuses
  the `=`, so it reads those regions as vite's own; the builder accepts them and `origin` credits chokidar / sirv.
  The bytes are chokidar's and sirv's code (apps-rollup 12,337 B chokidar; pop 5-6 KB sirv per set). The claim is
  right; the truth is wrong. Now declared in `docs/responsibility.md`.
- **marker-less inlined code, a truth gap.** vitepress's chunk carries `@polka/compression` (the `compression({
threshold = 1024, … })` middleware) and chalk with no `node_modules` marker at all, so the truth credits vitepress.
  Other packages' builds mark the same code, and `origin` indexes it under its real package (pop 1.5 KB each).
- **mixed lazy-init units, a granularity effect.** Bun turns vite's `logger.js` (picocolors region + vite's own
  constants) into ONE statement `var X = __esm(() => {…})`. The truth labels the statement by majority bytes:
  2,353 B vite, 1,956 B picocolors -> vite. The picocolors function nested inside it now has a picocolors row, and
  the structural rule names the statement picocolors (pop 8.6 KB per set). Under `drop` that nested function had no
  row and vite was named by its module.

**B. New wrong where the truth names another origin:** chains of inlining. `@mermaid-js/parser` inlines langium,
which inlines chevrotain, which inlines lodash-es; the truth credits the innermost origin, while the region the
claim came from names a middle link (chevrotain / langium) because that package's own build marks it. Likewise
devframe (`ohash` truth) claimed as c12, and vitepress (`import-meta-resolve`) claimed as prettier. 1-17 KB per set.

**Good -> unclaimed:** copies that r5 named by their carrier (mermaid for lodash-es in `@mermaid-js/parser`, vite
for convert-source-map / finalhandler, prettier for minimatch), judged good as the carrier's family or the
origin. With `origin` the same print is ALSO the origin package's row, the owners split across two families, and
the isolation / ambiguity rules leave it unclaimed. This is test2-esbuild's -17 KB named.

Both currents are small, generic and explained; A is mostly a truth gap (the copies-layer lane, exp129, derives
copies from installed files and may credit some of it). Neither is a reason to keep dropping, given the gains.

## 5. Full re-fingerprint

`/work/exp130-data/full/ix/k2-all-r5-origin`, built by this branch's `humanify-index` at `0a698c64`
(`build --sponge-regions origin`, keys `unit-rows/5+origin`):

- store: only k2-all-r5's literals shards (hard-linked; their key has no unit-rows version), so every module shard
  of all 68,304 versions was re-fingerprinted from the cached builds;
- the same seeded work dir as the lab (`/work/exp130-data/work`, a hard-link copy of `/work/exp124-data/work`, the
  work dir r5's own builds were made in: exp124's lesson);
- `flock /work/index-build.lock`, cores 34-63: 4.4 h of shards (124 chunks, all exit 0), 19 min merge at 34.6 GB
  peak; index id `c5fe9a7403c0…`.

Every index table (units, modules, literals, families, packages, versions, stoplists) is byte-identical to the
lab index's; only `manifest.json` differs (shard keys and the id). Scored on all 7 corpora (`*exp130-origin*`),
every set's summary equals the lab's (`*exp130-olab*`), so the table in §3 is the full index's too.

## Reproduce

```bash
S=experiments/130-sponge-origin/scripts
# lab: k2-all-r5's store minus the bundler-marker shards; work dir copied from /work/exp124-data/work
$S/lab-build.sh /work/exp130-data/lab <humanify-index, Origin keyed unit-rows/5> k2-all-r5-origin-lab origin
# full: a store holding only k2-all-r5's literals shards, the same work dir
$S/lab-build.sh /work/exp130-data/full <humanify-index at this branch> k2-all-r5-origin origin
$S/score-all.sh exp130-r5 /work/exp124-data/full/ix/k2-all-r5
$S/score-all.sh exp130-olab /work/exp130-data/lab/ix/k2-all-r5-origin-lab
python3 -I $S/table.py exp130-r5 exp130-olab
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
$PY -I $S/jdiff.py pop diff-pop.jsonl pop-exp124-r5 pop-exp124-orig && python3 -I $S/agg.py diff-pop.jsonl
```
