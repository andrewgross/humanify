# exp181 — the library index for every bundler we support

STATUS: BUILT and MEASURED (2026-11-02), branch `exp181-index-all-bundlers`,
cut from main `0e5893f77` (exp175 merged). Gate 13/13 in the worktree and
from a clean checkout. Warm neutrality NEUTRAL in names and off mode. Not
merged. Per-kind precision measured: **esbuild passes the 99.8% bar;
webpack and rollup do not** (Andrew decides whether they stay enabled).
App bytes claimed: 0 on every kind.

Andrew's directive (2026-11-01): "Our library interface should be usable for
all of the bundlers we support, we have discovery and the ability to specify
the bundler, that should mimic what we do here." Clarified: Bun was only the
best local test set, not a design choice, so no Bun-only paths, defaults or
wording remain in the library-index interface.

## Totals first

| kind (corpus)                  | bundles | right    | wrong   | app claimed | precision  | (shipper-only) | recall | vs 99.8% bar |
| ------------------------------ | ------- | -------- | ------- | ----------- | ---------- | -------------- | ------ | ------------ |
| Bun target=bun (bun-dev-bun)   | 45      | 86.06 MB | 39 KB   | 0           | **99.96%** | 99.48%         | 82.2%  | passes       |
| Bun target=node (bun-dev-node) | 45      | 93.37 MB | 276 KB  | 0           | **99.71%** | 99.00%         | 85.5%  | BELOW        |
| esbuild (kind-esbuild)         | 37      | 76.87 MB | 107 KB  | 0           | **99.86%** | 99.05%         | 85.8%  | passes       |
| webpack (kind-webpack)         | 34      | 59.38 MB | 319 KB  | 0           | **99.47%** | 99.32%         | 89.8%  | BELOW        |
| rollup (kind-rollup)           | 34      | 48.92 MB | 1179 KB | 0           | **97.70%** | 97.53%         | 81.9%  | BELOW        |

Precision is the scorer's headline (origin-aware truth, scorer 14); the
shipper-only column is the stricter truth. Labels `exp181-{bun,esbuild,webpack,rollup}`
under `experiments/096-library-matching-score/results/`.

- **webpack**: 78% of its wrong bytes (249 KB of 319 KB) are ONE bundle
  (vscode-github-actions); without it webpack reads 99.88%. 12 of 34 bundles
  have any wrong bytes.
- **rollup**: wrong bytes are spread: 30 of 34 bundles, the top three
  (vscode-github-actions 457 KB, electron-vite-starter 333 KB,
  prism-browser-community 302 KB) are 92% of it. Not one outlier.
- **Bun target=node** is below the bar too, on this bed with this index
  (exp150 measured 99.743 with the full `/3` index): not new, and not this
  lane's to change.

Nothing here changes what a run with an explicit index or a found index
claims: the matcher is untouched. This is what the RUN-TIME build makes
possible for the three kinds that could not build before.

## What changed

- **One tool owner, everything in the store** (`humanify_index::tools`, new):
  every recipe bundler's pinned tool lives at
  `<store>/tools/<bundler>-<version>/`, beside `registry/`, `store/` and the
  merged indexes, fetched there once and reused:
  - Bun 1.4.0 and esbuild 0.28.2 run as `node_modules/.bin/<bundler>` (the
    fetch places the platform binary over the package's placeholder, the
    step the skipped postinstall would do);
  - webpack 5.111.1 and rollup 4.64.5 (+ node-resolve 16.0.3, commonjs
    29.0.3, json 6.1.0, terser 1.0.0) run as the builder's own recipe
    scripts, embedded in the binary and written beside the install; a stale
    script in the store is rewritten (the script IS the recipe).
  - Every fetch is `npm install --before=2026-10-16`. Measured: webpack
    5.111.1 installed undated today pulls a newer minimizer plugin and fails
    every build (`EMBEDDED_LANGUAGES is not iterable`); dated, it builds.
  - The versions are the ones `/work/library-index/current` (as-of
    2026-10-15, the last multi-bundler index) recorded for those slices; the
    rollup plugins were recorded nowhere, so each is its newest release at
    that as-of.
  - Deleted: `cache::bun_binary`, `fetch_bun`, `PINNED_BUN`,
    `BUN_FETCH_BYTES`. No Bun-only tool path is left.
- **The run-time build builds every recipe kind**
  (`runtime_index::job`): the recipe the bundle needs
  (`DetectedRecipe::needed_recipe`, the same recipe the skip line hands the
  offline builder), run by the store's tool for its bundler, through the
  offline builder's own plan / build / merge (`cache::plan_build`,
  `build_planned`). One recipe per build: exp171's Bun build of both
  targets is gone (a Bun bundle's banner says its target). Deleted:
  `cannot_build_line` ("builds Bun indexes only") and `run_time_buildable`.
- **Kinds with no recipe say so** (browserify, parcel, unrecognised), with
  or without the flag:
  `no library index for parcel bundles in <store> — skipping library name matching (no index recipe exists for parcel bundles yet)`.
  Every recipe kind's skip line offers `--library-index-from-bundle`.
- **The notice stays, per kind**: it names the kind's tool
  (`bundled with esbuild 0.28.2 (just fetched into the store)`), the fetch
  is announced first with its size
  (`Library index: fetching esbuild 0.28.2 (~12 MB) into <store>/tools …`),
  and the estimate is the kind's own (`cache::version_cost`).
- **The bundler verdict is one verdict**: the index decision reads the
  toolchain the unpacker uses (detection, or forced `unpack.bundler` /
  `unpack.minifier`), read once in `unified::pipeline_body`. This already
  held after exp175; a test now pins it.
- The report trail's `pickedBun` is now `tool` (`"esbuild 0.28.2"`).
- Config: no new key or flag. Only the descriptions of
  `--library-index-from-bundle` / `library.index_from_bundle` changed (and
  the regenerated help golden and `humanify.example.toml`). **Overlap with
  exp179 (CLI interface): those two description strings** in
  `surface.rs` and `config/keys.rs`.
- Measurement plumbing: corpora `kind-esbuild`, `kind-webpack`,
  `kind-rollup` registered in `lib096.CORPORA`, `run.py` and the `eval`
  dispatcher's `--corpus` list (manifests and truth layers committed in
  `experiments/096-library-matching-score/`).

## What a user sees (real runs, debug binary, dead LLM endpoint)

esbuild bundle that reveals `ms@2.1.3`, empty working directory,
`--library-index-from-bundle` (19 s, network):

```
no library index for esbuild bundles — building one from this bundle into <cwd>/.humanify/library-index
Library index: fetching esbuild 0.28.2 (~12 MB) into <cwd>/.humanify/library-index/tools to build one (--library-matching off skips library names)
NOTICE: library names (the default) needs a library index for this bundle, and none was found.
  Building one now from the 1 npm package(s) this bundle reveals (1 to build, 0 already in the store): each is installed from the npm registry (install scripts skipped) and bundled with esbuild 0.28.2 (just fetched into the store).
  Estimated: … (from measured esbuild builds; the run waits for it, then goes on).
  …
Library index: built dae44e932f3a in 0 s — 1 version(s) indexed, 0 of them reused from the store; the store now takes 23 MB
Vendor identifier restoration: 19 identifiers restored across 1 vendor files (6 statements answered, 0 conflicts abstained)
```

`vendor/ms/index.js` gets `parse`, `fmtLong`, `fmtShort`, `options` back.
The same with a rollup bundle fetched rollup 4.64.5 and built its index.

## What the run-time build cannot see for webpack and rollup

Found on the way, NOT fixed here: the run-time build's seed list is what the
bundle REVEALS (library banners, inlined package.json). webpack and rollup
tree-shake an inlined package.json down to the fields used
(`require("ms/package.json").version` becomes `{version:"2.1.3"}`, no
name), and webpack's production terser moves `/*!` banners into a separate
LICENSE file. So on typical webpack/rollup bundles the run-time build finds
nothing to build from and prints the "reveals no npm package" skip line. The
build path itself works for both (proven with a bundle that reads the whole
package.json). Seeding from the discovery evidence (exp153's owner) is the
lever, per the existing note in `bundle_revealed_seeds`.

Also found: on this box a webpack bundle fails to unpack
(`read out/deobfuscated.js: No such file`, the webcrack shim) on main's
binary too. Not this lane's.

## How the precision was measured

- **Bed**: exp150's 45 bun-dev apps (the same installs, `--before` the
  index as-of), rebuilt by each kind with the store's pinned tools
  (`scripts/kind_corpus.py build`): esbuild with exp100's recipe (37 of 45
  built), webpack and rollup with the index's own recipe scripts (34 and 34;
  the failures are the apps' own resolution errors, `build.log`). Truth from
  the source map, frozen and layered exactly as exp150 did
  (`kind_corpus.py freeze`, exp150's `layers.sh` / `layers2.sh`). The app
  bundles are built with the SAME recipe the index uses: the favourable case
  for recall; precision is what this bed is for.
- **Index, built the run-time way per kind**: the run-time build's recipe
  for that kind, the store's pinned tool, `--names`, one build of every
  recipe sharing the installs, then one merge-only index per kind from the
  same shard store (`scripts/ix.sh`; 2.6 h for 3,377 pinned versions × 5
  slices, 3,286 packages indexed). Its package list is EVERY version the 45
  apps install (`kind_corpus.py pins`, read off disk): the run-time build
  seeded with a perfect reveal, the hardest case for precision (every
  candidate family is present). A real run-time index would be smaller.
- **Score**: `npm run eval -- library exp181-<kind> --index <ix-kind>
--rule structural --corpus kind-<kind> --commit a2569a83d`, scorer 14
  (`scripts/score.sh`); Bun: bun-dev against the Bun index built the same
  way.

What the run-time notice estimates per kind (`cache::version_cost`):
Bun's calibrated range (exp171) scaled by each kind's measured ratio on the
same 30 versions, built once per kind from an empty store
(`scripts/calib.sh`): disk 0.62 / 0.95 / 0.58 of Bun's, wall time
0.56 / 2.3 / 2.1 of Bun's (esbuild / webpack / rollup). Tool fetch sizes,
measured: Bun 81 MB, esbuild 12 MB, webpack 26 MB, rollup 13 MB.

## Tests (red first, then green)

- index `tools_test.rs`: every recipe bundler's tool lives in the store at
  its pin; webpack/rollup's script is the embedded bytes, a stale one is
  rewritten; the newest installed wins, none installed names the dated npm
  command; rollup's fetch pins its plugins; a recipe resolves to the store's
  tool. `fetches_every_tool_for_real` (`#[ignore]`, network) fetched all
  four and checked `--version`.
- index `cache_test.rs`: `each_kind_estimates_with_its_own_measured_cost`.
- cli `runtime_index_test.rs`: every recipe kind's default skip line offers
  the run-time build; browserify / parcel / unrecognised say "no index
  recipe exists … yet" with and without the flag; a bundle of every recipe
  kind that reveals no package is skipped saying so (no "Bun indexes only");
  the notice names each kind's own tool and no other kind's wording.
- cli `tests/pipeline_stages.rs`:
  - `a_forced_bundler_is_the_verdict_the_library_index_decision_reads`:
    detected Bun, then forced esbuild / webpack (+ forced minifier none) /
    rollup / parcel on the same bundle — the kind in the skip line, the
    recipe offered (`webpack:target=node,format=cjs` without `,minify` when
    the minifier is forced to none), the stats' kind, and the unpack
    stage's own `Bundle detection: bundler=<kind>` line agree.
  - `from_bundle_builds_an_esbuild_index_from_the_store_s_own_tool`: the
    hermetic run-time build of an esbuild bundle (fixture
    `tests/fixtures/runtime-build-esbuild/`, 40 KB: packument snapshots,
    the ms@2.1.3 shard for the esbuild recipe, and a stub esbuild that
    answers `--version` and refuses to build). Building line, no fetch,
    the shard reused, outcome `built` / kind `esbuild`, the sidecar's recipe
    and bundler version, and ms's names restored under `vendor/ms/`. The
    fixture is prettier-ignored: the commit hook had reformatted the
    minified bundle, which the clean-checkout gate caught.
- `test/eval-library-args.test.ts`: the dispatcher accepts `kind-esbuild`.

The forced-bundler test went green on the first run for everything except
parcel's line: forcing was already one verdict after exp175; what was red
was the no-recipe wording.

## Proof

| check                                                  | result                                                                      |
| ------------------------------------------------------ | --------------------------------------------------------------------------- |
| `npm run check`, worktree (before commit)              | ALL 13 STAGES PASSED                                                        |
| `npm run check`, clean checkout `a2569a83d`            | FAILED rust:unit: the hook-reformatted fixture (fixed in `a0e498935`)       |
| `npm run check`, clean checkout `a0e498935`            | ALL 13 STAGES PASSED                                                        |
| neutrality names (both legs `library.index=cb64c1f5…`) | NEUTRAL: 0 files / 0 lines, exit 0 / 0, cache candidate +0, baseline **+0** |
| neutrality off (`--pipeline-set library.matching=off`) | NEUTRAL: 0 files / 0 lines, exit 0 / 0, cache candidate +0, baseline **+0** |

Neutrality: WARM, 2.1.85→2.1.86 vs main `0e5893f77`, launched as exp175's
(`--priors /work/exp050-cold --cache /work/neutrality-cache`, own
`--workdir`). Claude Code is a Bun bundle and the eval passes its index
explicitly, so nothing here can reach it.

## What this does NOT prove

- Names-mode output on webpack/rollup bundles end to end: the claims are
  scored (`discover`, the same claims names mode applies), but the webpack
  unpack fails on this box (above), and rollup has no module wrappers to
  place vendor files by.
- A smaller, really-revealed run-time index per bundle: precision was
  measured against the full installed closure, which is the stricter test.
- The offline builder still takes its tools as flags; pointing it at a
  store's `tools/` is by hand.

## Proposed CLAUDE.md text (for Andrew)

After exp175's proposed sentence, replace "for Bun bundles only" with:

> `--library-index-from-bundle` builds one at run time for every kind the
> index has a recipe for (Bun, esbuild, webpack, rollup — exp181), with the
> pinned tool for that kind fetched into `<store>/tools/`; a kind with no
> recipe yet is skipped saying so. Measured precision on exp181's bed:
> esbuild 99.86%, webpack 99.47%, rollup 97.70% (Bun 99.96 / 99.71), app 0.
