# exp115: vendored-copy file paths in the index (index /4) + `vendored_presence`

## STATUS (2026-10-15)

DONE on branch `exp115-vendored-paths` (off main `b4db2b76`; main `bc59bd57` (exp112) and `126dd3a9` (exp116) merged
in). NOT merged. Gate: ALL 13 STAGES PASSED (bare) at `6ac52484`, after each merge, and at the branch head. Warm
neutrality: NEUTRAL 0/0, cache +0/+0, exit 0/0 at `6ac52484` vs `b4db2b76`. The same result at the branch head `474eb6f1` vs main
`126dd3a9`: NEUTRAL, 0/0, +0/+0. Every number below is scorer `exp096-score/7`, the structural rule, and the
k2-l12m4 index (Bun/esbuild) or step4-wr (webpack/rollup), marked by the lab copy of the merge rule (see "Index").

- **What it is.** The index merge now marks every unit row whose FILE, inside its owner's package, is a copy of
  another package (`humanify_core::libindex::vendored`). The format is bumped to `humanify-library-index/4`: the
  unit lookup row carries the mark, and the loader reads only /4. A new key `[library.discovery]
vendored_presence` (default OFF) drops a package whose every index seed stands only on marked rows. With
  `presence_fence` on, nothing names what it drops.
- **Ceiling: small.** Path evidence covers only about 10-20% of exp114's "shipper named for its dependency's code"
  class. Most shippers keep their dependency's code in their own entry chunks (`build/index.js`,
  `agent-<hash>.js`, `chunk-<hash>.js`, `manager.mjs`) or in a file whose name is not the origin's
  (`wasm/tree-sitter.js` vs web-tree-sitter, confbox `dist/toml.cjs` vs smol-toml). The path names nothing there.
- **Measured (F0.5).**
  - Held-out: 90.18 -> 90.26 with `vp`; 90.29 -> **90.34** with allf+vp. Precision 99.24 -> 99.33; wrong-family
    154.8 -> 136.2 KB.
  - task-master, wrangler, Claude Code, apps-esbuild, dev-webpack and dev-rollup: unchanged.
  - pop: +0.10 / +0.20 (vpf vs shipped); +0.00 / +0.01 (allf+vp vs allf).
  - test, unfenced `vp` vs shipped: bun +0.73, esbuild +0.13.
  - test, allf+vp vs allf: -0.01 to 0.
  - test2 (summaries only): +0.00 / +0.01.
  - App claimed is 0 everywhere.
- **Recommendation.** Keep it default-off. If a default flip is wanted, flip `vendored_presence` WITHOUT the
  fence: scorer 7 credits origins, and the unfenced form is the best on test. The fence (which exp114's allf
  needs) mutes it to about zero on test.
- **Not reached:** the 99.8% bar on any set but Claude Code.

## Totals (P / R / **F0.5**; scorer 7)

| config                     | heldout                   | task-master               | wrangler                  | cc                         | apps-esbuild              | dev-webpack               | dev-rollup                | test-bun                  | test-esbuild              | test-webpack              | test-rollup               |
| -------------------------- | ------------------------- | ------------------------- | ------------------------- | -------------------------- | ------------------------- | ------------------------- | ------------------------- | ------------------------- | ------------------------- | ------------------------- | ------------------------- |
| shipped (exp109)           | 99.09 / 66.33 / **90.18** | 95.69 / 65.36 / **87.57** | 95.99 / 18.52 / **52.26** | 100.00 / 81.90 / **95.77** | 98.53 / 89.70 / **96.63** | 99.59 / 59.24 / **87.65** | 99.84 / 71.14 / **92.39** | 95.44 / 66.31 / **87.73** | 95.86 / 75.86 / **91.06** | 97.95 / 39.02 / **75.23** | 94.62 / 35.59 / **71.05** |
| shipped + vp               | 99.20 / 66.34 / **90.26** | =                         | =                         | =                          | 98.53 / 89.70 / **96.63** | =                         | =                         | 95.75 / 67.82 / **88.46** | 95.95 / 76.08 / **91.19** | 97.96 / 39.02 / **75.23** | =                         |
| shipped + vp + fence (vpf) | 99.20 / 66.31 / **90.25** | =                         | =                         | =                          | 98.61 / 89.69 / **96.69** | -                         | -                         | 95.66 / 66.30 / **87.87** | 95.97 / 75.86 / **91.14** | 97.96 / 39.02 / **75.23** | =                         |
| allf (exp114)              | 99.24 / 66.35 / **90.29** | 96.24 / 65.39 / **87.94** | =                         | =                          | 98.73 / 89.71 / **96.78** | 99.62 / 59.26 / **87.68** | =                         | 95.78 / 66.36 / **87.98** | 96.02 / 75.89 / **91.18** | 98.15 / 39.03 / **75.33** | 94.92 / 35.70 / **71.27** |
| allf + vp                  | 99.33 / 66.33 / **90.34** | 96.24 / 65.39 / **87.94** | =                         | =                          | 98.73 / 89.70 / **96.78** | 99.62 / 59.26 / **87.68** | =                         | 95.77 / 66.35 / **87.97** | 96.02 / 75.88 / **91.18** | 98.15 / 39.02 / **75.32** | 94.92 / 35.70 / **71.27** |

Wrong-family bytes, held-out: shipped 185,893 -> vp 161,402; allf 154,754 -> allf+vp 136,169. Test-bun: shipped
2,068,971 -> vp 1,965,565.

**pop** (exp112's dev corpus, same population as test): shipped 91.32 / 76.75 / **87.98** and 90.47 / 79.00 /
**87.92** (bun / esbuild); vpf **88.08** / **88.12**; allf **88.25** / **88.27**; allf+vp **88.25** / **88.28**.

**test2** (exp116, summaries only; before = `test2-main-k2-structural`): bun / esbuild F0.5 93.50 / 93.07 -> vp
93.50 / 93.08, vpf 93.50 / 93.08, allf 93.64 / 93.08, allf+vp 93.64 / 93.08. Precision 98.52 / 99.41 (allf 98.71).

`vp` = `vendored_presence = true`; `fence` = `presence_fence = true`; `allf` = exp114's context_presence 1.5 / 16384 +
shadow_ties + presence_fence. The test, pop and test2 numbers were not diagnosed. Hand-reads below are from dev
corpora only.

## 1. Measure first: how many rows sit in a vendored-copy file (`pathcount.py`, `marks.py`)

The builder already drops rows under a plain `vendor/`, `vendored/` or `third_party/` directory, and rows of files
behind a bundler module marker (`sponge_evidence`, unit-rows/4). What reaches the k2 index:

- **Raw grammar** (directory or file stem naming any known package): 342 K / 324 K / 280 K rows of 6.4 M / 6.0 M /
  5.8 M (bun-bun / bun-node / esbuild). The bare file-stem rule is mostly noise: `base`, `color`, `table`,
  `config`, `events-<hash>.js` (react-three-fiber's own events module), babylonjs' `babylon.js`.
- **After confirmation**, the shipped rule marks 147,685 / 140,984 / 103,275 rows (1,088 / 980 / 571 files). For a
  weak path (a `libs/` / `deps/` / `external/` directory, or the file's stem with its chunk hash stripped), the
  named package must itself hold at least 2 and at least 10% of the file's prints. Strong directories mark as they
  stand: `compiled/<pkg>/`, `*-vendor/<pkg>/`, `vendors/`, `bundled/`, `node_modules/<pkg>/`.
- **Largest marked files:** @ts-morph/common `dist/typescript.js`; npm-check-updates
  `build/chunks/typescript-<hash>.cjs`; prettier `plugins/{typescript,postcss,yaml,acorn,meriyah,graphql}.mjs`;
  @rspack/core `compiled/*`; mermaid `dist/katex-<hash>.js`; chromatic `dist/{meow,execa}-<hash>.cjs`; confbox
  `dist/_chunks/libs/*`; crossws `dist/_chunks/libs/ws.mjs`; @antv/vendor `lib-vendor/d3-*`.

**Ceiling on exp114's wrong bytes** (`ceiling.py`, from the shipped labels' claims):

| set             | wrong (base) | package-level drop: wrong removed | good removed | seed-level drop: wrong removed | good removed  |
| --------------- | ------------ | --------------------------------- | ------------ | ------------------------------ | ------------- |
| held-out        | 185,927      | 26,631                            | 0            | 8,262                          | 3,246         |
| held-out (allf) | 154,788      | 18,582                            | 0            | 1,070                          | 3,246         |
| task-master     | 144,730      | 0                                 | 0            | 0                              | 0             |
| wrangler        | 5,652        | 0                                 | 0            | 0                              | 0             |
| apps-esbuild    | 342,852      | 25,427                            | 0            | 5,926                          | **1,303,257** |

The seed-level drop refuses every seed that stands on marked rows. On apps it loses 1.3 MB of RIGHT prettier
claims: an app that bundles prettier is credited with prettier's own `plugins/typescript.mjs`, a vendored copy of
TypeScript. So the rule must be PACKAGE-level: a package is dropped only when the bundle shows none of its own
code. That variant removed 0 good bytes in the lab. The ceiling, about 18-27 KB on held-out and 25 KB on apps, is
a small share of exp114's class A (88 / 137 / 264 KB). Path evidence simply does not exist for most of it:

| class A shipper                             | its copy lives in                                                                      | marked?                             |
| ------------------------------------------- | -------------------------------------------------------------------------------------- | ----------------------------------- |
| npm-check-updates <- zod                    | `build/index.js`                                                                       | no (its typescript chunk is marked) |
| @chromatic-com/storybook <- zod             | `dist/manager.mjs`                                                                     | no                                  |
| @vscode/tree-sitter-wasm <- web-tree-sitter | `wasm/tree-sitter.js` (`tree-sitter` is another package, which does not hold the code) | no                                  |
| crossws <- ws                               | `dist/_chunks/libs/ws.mjs`                                                             | yes                                 |
| @prisma/fetch-engine <- picomatch           | `dist/chunk-<hash>.js`                                                                 | no                                  |
| @mastra/core <- ai                          | `dist/agent-<hash>.js`                                                                 | no                                  |
| @livekit/components-react <- rxjs           | `dist/contexts-<hash>.mjs`                                                             | no                                  |
| @antv/vendor <- d3                          | `lib-vendor/d3-*/`                                                                     | yes                                 |
| @rspack/core <- tinypool                    | `compiled/tinypool/`                                                                   | yes                                 |

## 2. What was built

- `crates/humanify-core/src/libindex/vendored.rs`: the path grammar (`named_package`) and the slice post-pass
  (`mark_vendored`). Tests are in `vendored_test.rs`, written red first.
- `merge.rs` (humanify-index): after the unit rows, marks every slice. The known names are the index's owners; the
  families are the shipped ones plus the derived links among those owners. It writes `vendored.json` (rows and
  files marked per slice). `merge_test.rs` covers it.
- `format.rs` / `table.rs`: `UnitRow.vendored: Option<String>` (row table, omitted when None). The unit lookup row
  grows from 12 to 16 bytes: owner, module bytes, then a mark u32 holding the named package's owner id,
  `VENDORED_UNINDEXED`, or `NOT_VENDORED`. Magic `HXUNIT04`, format `/4`. `UnitHit::is_vendored`.
- `discovery.rs`: `DiscoveryOptions::vendored_presence`, with `Containment::vendored_only(u, P)`. P holds at least
  one of u's prints within the specificity bound, and every such row of P is marked. It runs in `presence()`: a
  package (family key) with no declared seed and no index seed off marked rows is dropped, and fenced when
  `presence_fence` is on.
- Config key `[library.discovery] vendored_presence`, plus docs and `humanify.example.toml`.

The Rust rule is STRICTER than the lab's package-level emulation. It reads every print of every index seed (also
the seeds that later lose to a declaration or a rival) and every row the package holds. confbox on apps prisma
therefore survives: some of its seeds stand on old versions' unmarked `dist/toml.cjs`, which is smol-toml code
under a name that is not the package's. That is why apps-esbuild moves -516 B instead of the ceiling's -25 KB.

## 3. Index

The scoring index is `/work/exp115-data/ix/k2-l12m4-vp-lab`, plus step4-wr at `/work/exp115-data/ix/step4-wr-vp-lab`.
Both come from `lab-mark/`, a crate outside the workspace. It reads the /3 index's own unit rows, applies
`humanify_core::libindex::vendored::mark_vendored` (the merge's own function, with the same names and families),
re-encodes them with humanify-core's encoder and writes a /4 manifest. With no key on, the marked index gives
byte-identical summaries to the /3 one: `exp115-base-structural` = `exp109-final-k2-structural` on every set.

The real `--merge-only` re-merge of exp108's k2 store, to `/work/exp115-data/ix/k2-l12m4-vp`
(`/work/exp115-data/merge-k2.sh`, binary `humanify-index` @ `6ac52484`), is queued under `/work/index-build.lock`.
exp113's k2-all build has held that lock all night. When the merge runs, its `units.*.{jsonl,bin}` should be
byte-identical to the lab index's (same rows, same rule). Log: `/work/exp115-data/logs/merge-k2-l12m4-vp.out`.

## 4. Hand-reads (dev corpora; `../114-dev-precision/handread.py`, one claim per (bundle, old, new) group)

| #   | config  | bundle                  | was                              | now                | truth                                | verdict                                                                                   |
| --- | ------- | ----------------------- | -------------------------------- | ------------------ | ------------------------------------ | ----------------------------------------------------------------------------------------- |
| 1   | vp      | held-out task-master    | openai (seed)                    | zod-to-json-schema | zod-to-json-schema `parsers/date.js` | fixed: the `dateStrategy` parser                                                          |
| 2   | vp      | held-out gitbook-reader | @antv/vendor (seed)              | d3-timer           | d3-timer `src/timer.js`              | fixed: d3-timer's `wake`/`poke`                                                           |
| 3   | vp      | held-out docusaurus     | @rspack/core (module, 6.3 KB)    | unclaimed          | tinypool `dist/index.js`             | wrong removed: tinypool's Pool class                                                      |
| 4   | vp      | held-out codex          | @antv/vendor (seed)              | unclaimed          | d3-dispatch `src/dispatch.js`        | wrong removed (d3-dispatch left open)                                                     |
| 5   | vp      | held-out codex          | @antv/vendor (seed)              | d3-dsv             | d3-dsv `src/dsv.js`                  | fixed: `formatDate`                                                                       |
| 6   | vp      | held-out gitbook-reader | @antv/vendor (seed)              | unclaimed          | d3-dispatch                          | wrong removed                                                                             |
| 7   | vp      | held-out vite           | @umijs/deps (seed)               | unclaimed          | `<self>`, origin on-finished         | wrong removed: on-finished in vite's own dist                                             |
| 8   | vp      | held-out docusaurus     | @umijs/utils (seed)              | unclaimed          | find-up `index.js`                   | wrong removed: `findUp`                                                                   |
| 9   | vp      | held-out gitbook-reader | @antv/vendor (seed)              | d3-dsv             | d3-dsv                               | fixed                                                                                     |
| 10  | vp      | held-out codex          | @antv/vendor (seed)              | d3-timer           | d3-timer                             | fixed                                                                                     |
| 11  | vp      | held-out task-master    | @langchain/core (module)         | zod-to-json-schema | `errorMessages.js`                   | fixed: `setResponseValueAndErrors`                                                        |
| 12  | allf+vp | held-out codex          | d3-hierarchy (module, RIGHT)     | unclaimed          | d3-hierarchy `cluster.js`            | COST: the fence holds @antv/vendor's freed unit, so d3-hierarchy cannot propagate into it |
| 13  | allf+vp | held-out task-master    | zod-to-json-schema (seed, RIGHT) | unclaimed          | `parsers/intersection.js`            | COST: fenced                                                                              |
| 14  | vpf     | apps prisma             | confbox (down)                   | unclaimed          | prettier, origin json5               | wrong removed: json5's unicode tables                                                     |
| 15  | vpf     | apps shadcn             | openai (seed)                    | unclaimed          | zod-to-json-schema `parsers/date.js` | wrong removed                                                                             |
| 16  | vpf     | apps shadcn             | zod-to-json-schema (down, RIGHT) | unclaimed          | `parsers/intersection.js`            | COST: fenced                                                                              |

Totals: 13 of 16 changes are real fixes or real wrong claims removed. The other 3 are right claims lost, all to the
FENCE: the unit P dropped is claimed by no one, so the origin cannot propagate into it. Unfenced `vp` turns those
into right claims (1, 2, 5, 9-11), which is also why the unfenced form gains the most on test under scorer 7.

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
$PY -I pathcount.py /work/exp108-data/idx/ix/k2-l12m4 <slice> out.json        # raw grammar counts
$PY -I marks.py /work/exp108-data/idx/ix/k2-l12m4 <slice> /work/exp115-data/marks-k2-<slice>
$PY -I ceiling.py exp109-final-k2-structural /work/exp115-data/marks-k2         # [--corpus apps <apps label>]
CARGO_TARGET_DIR=/work/exp115-data/lab-target cargo build --release --manifest-path lab-mark/Cargo.toml
exp115-lab-mark /work/exp108-data/idx/ix/k2-l12m4 /work/exp115-data/ix/k2-l12m4-vp-lab
VP="--pipeline-set library.discovery.vendored_presence=true"; F="--pipeline-set library.discovery.presence_fence=true"
npm run eval -- library exp115-vp-structural --index /work/exp115-data/ix/k2-l12m4-vp-lab --rule structural $VP
# scripts: /work/exp115-data/score{1..5}.sh (dev, apps, bundlers, test, pop, test2), table.py
```

Labels -> commits: `exp115-*`, `test-exp115-*` at `6ac52484`; `pop-exp115-*`, `test2-exp115-*` at `42d2a087`
(the same crates). All are re-scored under scorer 7.
