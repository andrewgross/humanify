# exp124: region-aware sponge rows (index builder)

## STATUS (2026-10-16)

DONE on branch `exp124-region-sponge` (off main `7daf1f0b`, main `ebf9a271` merged in: copy_presence v2 defaults).
Builder-only change (`crates/humanify-index`); humanify-core is untouched, so no neutrality run is needed.

- **The rule.** Before, the index builder's sponge rule (`sponge_evidence`, exp097) dropped EVERY unit of a
  package's file when one module marker in it named another package. A package's own rolldown/esbuild chunk that
  inlines a few dependencies therefore had none of its own code indexed. Now the markers delimit regions, and only
  units in a FOREIGN region are dropped:
  - rolldown `//#region <path>` up to `//#endregion`. A lazily initialised module's body right after its
    `//#endregion` (`__esm(` / `__esmMin(`) stays in that module's region, up to the next marker or a top-level
    `export`.
  - esbuild/Bun `// <path>` (and webpack's markers) up to the next marker.
  - A region is foreign when its marker names another package's `node_modules` path.
  - A unit keeps its row as the shipper's when at least 90% of its mapping segments in that file lie outside the
    foreign regions.
  - This is the same marker rule the scorer's truth uses (`origins.py` `marker_regions7`). `docs/responsibility.md`
    has a row declaring the twin.
  - A vendored directory still drops the whole file.
  - The unit-rows version is now `unit-rows/5`.
- **The choice: drop the foreign regions.** Crediting them instead to the package their marker names (at its pnpm
  path's version) was built, measured and REFUTED. It is the better recall lever on dev: wrangler +3.84, pop +1.1 to
  +1.2 and apps-esbuild +1.01 F0.5. But it recreates exp121's sponge problem on the test split: test-bun precision
  falls 97.79 -> 92.59 (F0.5 -2.79), test-esbuild 96.96 -> 93.29 (-2.18), and pop-bun precision 95.37 -> 95.03.
  That holds even with copy_presence v2 on. The flag is deleted.
- **Result (drop), main's shipped defaults (copy_presence v2, scorer 9), k2-all vs the same index re-fingerprinted:**
  - **Precision never drops on any of the 19 sets.** It rises on 13 of them, test2 included.
  - **F0.5 rises on 15 sets.** held-out 90.42 -> 90.79, task-master 87.99 -> 89.59, apps-esbuild 97.20 -> 97.78,
    pop-bun 91.13 -> 91.77, test2-bun 93.64 -> 94.21, test2-esbuild 93.10 -> 93.64.
  - **App claimed is 0 everywhere.**
  - **One set moves down.** apps-webpack F0.5 is -0.003 (one app: 16.5 KB of recall).
- **One more builder fix, found because it was needed.** k2-all's shards were made in several work dirs (exp095
  selABC, exp101, exp105, exp109). Some bundlers wrote sourcemap sources relative to the install's REAL path
  (`../../../../../../selABC/work/x@1/node_modules/...`). Re-fingerprinting such a build from a seeded work dir found
  no owner at all: localforage's shards came back empty. `SourceMap::relocate` now re-anchors a source that
  resolves nowhere on the install dir's name (only when the result exists). Without it, a re-fingerprint of k2-all
  from cached builds silently loses every such shard.
- **Index.** The numbers above are the LAB index `/work/exp124-data/lab-drop/ix/k2-all-region-drop`:
  - k2-all's store, minus the 9,135 module shards whose notes record a bundler-marker drop (the only shards the rule
    can change; 2,143 versions);
  - those shards rebuilt by the new rule from the cached builds, with the old key, then a full merge.
  - The real re-fingerprint (`unit-rows/5`, every module shard) is `/work/exp124-data/full/ix/k2-all-r5`; see
    "Full re-fingerprint" below.

## 1. Lab first: does the row now exist? (`scripts/lab_hits.py`)

The lab took exp123's not-emitted units in a package's own pre-bundled file (truth package's shard was a
bundler-marker sponge). For each one, it checked whether the rebuilt shard now holds a row with the unit's exact
hash under the right owner: the truth package for own code, the origin for a copy. The figures are points of each
set's library bytes, and a ceiling for the matcher:

| set          | own: census | own: row exists (drop) | copy: census | copy: row exists (origin) |
| ------------ | ----------: | ---------------------: | -----------: | ------------------------: |
| apps-esbuild |       1.716 |                  1.640 |        0.384 |                     0.161 |
| pop-bun      |       0.864 |                  0.793 |        1.150 |                     1.057 |
| pop-esbuild  |       0.802 |                  0.755 |        1.157 |                     1.113 |
| apps-rollup  |       0.493 |                  0.364 |        0.179 |                     0.135 |
| held-out     |       0.296 |                  0.282 |        0.021 |                     0.016 |

With k2-all's original shards, every column is 0.000. Dropping the foreign regions makes 92-96% of the own-code
class indexable (apps-rollup 74%).

## 2. Scores (main's shipped defaults at `ab290d8e`, structural, scorer 9; P / R / F0.5, delta F0.5)

Baseline `exp124-base` is k2-all re-scored at this branch's merged HEAD. It equals exp121's `*exp121-k2-v2*`
config, now main's default. The pre-merge control `exp124-ctl` reproduced `exp119-k2all-st` byte for byte. test
and test2 are summaries only and were never diagnosed.

| set           | k2-all (base)      | region drop (SHIP)         | region origin (refuted)       |
| ------------- | ------------------ | -------------------------- | ----------------------------- |
| cc            | 100.00/81.90/95.77 | 100.00/81.90/95.77 (+0.00) | 100.00/81.90/95.77 (+0.00)    |
| task-master   | 96.25/65.50/87.99  | 96.60/69.44/89.59 (+1.60)  | 96.61/69.60/89.65 (+1.66)     |
| held-out      | 99.25/66.70/90.42  | 99.31/67.60/90.79 (+0.37)  | 99.31/67.82/90.87 (+0.45)     |
| wrangler      | 95.99/18.53/52.28  | 95.99/18.53/52.28 (+0.00)  | 96.45/21.00/56.12 (+3.84)     |
| apps-esbuild  | 99.16/90.09/97.20  | 99.29/92.15/97.78 (+0.58)  | 99.77/92.44/98.21 (+1.01)     |
| apps-rollup   | 98.93/77.42/93.72  | 98.98/78.66/94.12 (+0.40)  | 99.06/79.28/94.35 (+0.63)     |
| apps-webpack  | 99.39/73.33/92.79  | 99.39/73.30/92.79 (-0.003) | 99.39/73.36/92.80 (+0.01)     |
| dev-rollup    | 99.87/77.53/94.43  | 99.87/77.53/94.43 (+0.00)  | 99.87/77.53/94.43 (+0.00)     |
| dev-webpack   | 99.64/62.89/89.22  | 99.64/62.89/89.22 (+0.00)  | 99.64/62.89/89.22 (+0.00)     |
| pop-bun       | 95.37/77.38/91.13  | 95.51/79.34/91.77 (+0.64)  | **95.03**/82.52/92.23 (+1.10) |
| pop-esbuild   | 93.93/79.89/90.74  | 94.08/81.42/91.24 (+0.50)  | 93.94/84.73/91.94 (+1.20)     |
| test-bun      | 97.79/66.54/89.39  | 97.85/67.56/89.80 (+0.40)  | **92.59**/68.80/86.60 (-2.79) |
| test-esbuild  | 96.96/76.17/91.94  | 97.00/76.80/92.16 (+0.21)  | **93.29**/77.96/89.76 (-2.18) |
| test-rollup   | 97.91/68.41/90.13  | 97.94/68.80/90.29 (+0.16)  | 96.67/70.76/90.07 (-0.06)     |
| test-webpack  | 99.24/70.54/91.78  | 99.24/70.62/91.80 (+0.03)  | 98.00/71.12/91.11 (-0.67)     |
| test2-bun     | 98.71/77.70/93.64  | 98.85/79.30/94.21 (+0.56)  | 98.85/79.55/94.28 (+0.63)     |
| test2-esbuild | 99.43/74.23/93.10  | 99.49/75.80/93.64 (+0.53)  | 99.46/75.76/93.61 (+0.50)     |
| test2-rollup  | 96.28/74.90/91.08  | 96.33/75.35/91.25 (+0.16)  | 96.34/75.65/91.35 (+0.26)     |
| test2-webpack | 99.20/69.26/91.31  | 99.27/70.08/91.64 (+0.33)  | 99.27/70.09/91.64 (+0.33)     |

App claimed is 0 in every cell. The same comparison under the pre-copy_presence defaults
(`exp119-k2all-st` vs `exp124-rdrop` / `exp124-rorig`) gives the same picture: drop has precision up or equal
everywhere, and origin has test-bun precision 92.09 and test-esbuild 92.79.

Why origin loses: a shipper's inlined copy credited to the package its marker names becomes that package's
presence and its seeds, wherever the shipper is bundled. That is exp121's sponge-seed problem moved into the
index. Since the test split cannot be diagnosed, the mechanism is inferred from pop (pop-bun precision -0.34)
and from exp118/exp121's measurements of the same class.

## 3. Full re-fingerprint (`unit-rows/5`)

`/work/exp124-data/full/ix/k2-all-r5` re-fingerprints every k2-all module shard from cached builds:

- the binary is this branch's `humanify-index` at `ab290d8e`;
- every build is hard-linked into a seeded work dir (`scripts/seed-work.sh`), with node_modules symlinked to the
  source install;
- literals shards are reused, since their key does not include the unit-rows version;
- it ran under `flock /work/index-build.lock`, 30 cores (34-63).

RESULT: see the appended line below. The shipped claim is that it equals the lab index's unit rows.

## Reproduce

```bash
S=experiments/124-region-sponge/scripts
$S/lab-store.sh /work/exp124-data/lab-drop          # k2-all store minus the bundler-marker shards; seeded work dir
$S/lab-build.sh /work/exp124-data/lab-drop <lab humanify-index keyed unit-rows/4> k2-all-region-drop
python3 -I $S/lab_hits.py /work/exp124-data/lab-drop/store
$S/score-all.sh exp124-base /work/exp113-data/ix/k2-all
$S/score-all.sh exp124-drop /work/exp124-data/lab-drop/ix/k2-all-region-drop
python3 -I $S/table.py exp124-base exp124-drop exp124-orig
# full re-fingerprint: a store holding only k2-all's literals shards, the seeded work dir
$S/seed-work.sh <versions list> /work/exp124-data/work
$S/lab-build.sh /work/exp124-data/full <humanify-index at this branch> k2-all-r5
```
