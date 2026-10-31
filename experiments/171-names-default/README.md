# exp171 — names mode is the default

STATUS: BUILT (2026-10-31), branch `exp171-names-default` off main
`dba4a0e10`. Andrew's decision (2026-10-31): `--library-matching names` is
the default, with a clear notice before the pipeline builds a library index.
The reference repeats are in "The new reference" below.

## Totals first

- **The flip.** `library.matching` defaults to `names` and
  `library.index_build` to `auto` (one owner: the key registry,
  `crates/humanify-cli/src/config/keys.rs`). The opt-out is the existing flag,
  `--library-matching off`. `library.names_unlocked` is deleted; a file that
  sets it is refused as an unknown key.
- **What a default run does with no `--library-index`.** It searches its
  store (`.humanify/library-index` under the output dir). Then:
  - an index is FOUND: one line, no build, no notice;
  - nothing can serve the bundle (not Bun-built, or it reveals no npm package
    name): one line, and the run goes on as `off`, byte for byte;
  - otherwise it BUILDS one, after a NOTICE with the estimate, the location
    and the opt-out. It does not ask for confirmation, because a prompt would
    hang batch runs (Andrew's rule "if the index is not ready, wait" means
    build it).
- **The big finding: the run-time build serves almost nothing yet.** The build
  seeds only from what the bundle reveals: library banner comments and inlined
  `package.json`s. **Claude Code 2.1.216 reveals no npm package name**: two
  banners, both `azure/msal-…` without the `@`. So a default run on Claude
  Code with no prebuilt index is INERT: it runs as `off`. Names mode on Claude
  Code needs the prebuilt full index (43 GB, `cb64c1f5…`). Of the bun-dev
  apps tried, doctoc and helm-intellisense reveal nothing (inert); better-svg
  reveals 5, novel-engine 2 and array-index-inlay 15. Most of these are
  linter/tooling banners, and some of them fail to install.
- **The eval never uses the run-time build.** `npm run eval` hands every
  names-mode launch the full prebuilt index (pairs.json `libraryIndex`, or
  `--library-index`). Its as-of would otherwise be the day of the run.
- **Two names-mode defects the flip exposed, both reported and neither hidden:**
  1. The run-time build was fail-hard: one odd package killed the run (a
     banner URL sent to `npm view`; a peer-dependency install failure; a pin
     the plan refused). It is now fail-soft per version: dropped and failed
     versions are recorded in `build.json` and in the closing line, and only an
     empty index fails.
  2. **Names-mode vendor naming defeats the vendor content carry (finding #90).**
     exp169 names vendored factory bodies, so the PRIOR tree's vendor files
     carry descriptive identifiers while the fresh factory text is raw. The
     content carry shingles identifier tokens, so a CHANGED vendored module no
     longer pairs with its prior file and draws a new path. The e2e fixture
     `bun-text-assets` catches this. It is declared as a KNOWN GAP (it must
     fail exactly so; the fix must delete the entry). Off mode is unaffected.
     Follow-up lane: pair on identifier-blind text, or on the prior's raw
     text.

## The notice, as a user sees it

On a Bun bundle with an empty store and no Bun (better-svg, measured):

```
Library index: none in <out>/.humanify/library-index serves this bundle — fetching Bun 1.4.0 (~82 MB) into it to build one (--library-matching off skips library names)
NOTICE: library names (the default) needs a library index for this bundle, and none was found.
  Building one now from the 5 npm package(s) this bundle reveals (5 to build, 0 already in the store): each is installed from the npm registry (install scripts skipped) and bundled with Bun 1.4.0 (just fetched into the store).
  Estimated: 60 MB to 200 MB of disk, 20 s to 85 s (measured on Bun-built apps; the run waits for it, then goes on).
  Stored in: <out>/.humanify/library-index (later runs with this store reuse it).
  To skip it: --library-matching off (no library names), or --library-index <dir|id> to use a prebuilt index.
  Skipped 1 name(s) the bundle offered that are not npm packages: http://mths.be/fromcodepoint
Library index: built 40b9d1ed150f in 4 s — 2 version(s) indexed, 0 of them reused from the store, 3 could not be indexed (@typescript-eslint/eslint-plugin@8.68.0, @typescript-eslint/parser@8.68.0, @typescript-eslint/typescript-estree@8.68.0); the store now takes 108 MB
```

Other paths:

```
Library index: using 40b9d1ed150f from <store> (found; nothing to build)
Library index: assembling one from 2 version(s) already in <store> (nothing to download or build)
Library names: off for this run — no library index in <store> serves this bundle, and it reveals no npm package name to build one from (its library banners and inlined package.json files name none). Pass --library-index <dir|id> to use a prebuilt index, or --library-matching off to silence this
Library names: off for this run — this bundle was built with esbuild, and a library index can only be built for Bun bundles so far (pass --library-index <dir|id> with an index that has its slice, or --library-matching off to silence this)
```

The estimate is `estimate_build` (`crates/humanify-index/src/cache.rs`): the
number of versions to BUILD (reused ones cost nothing), times a per-version
range calibrated below. A found index that a run-time build made for OTHER
packages is not reused. Every package the bundle reveals must be one that its
build planned or dropped (`uncovered`); otherwise app A's index would serve
app B forever. The prebuilt full index has no seed record and covers
everything.

## What the index costs (measured)

`measure-index.mts` / `measure-batch.sh`: one stub-LLM run per row. The
wall time is the whole run, peak memory is the whole process tree (the
binary plus npm/bun children, sampled from /proc), and disk is the store
after the run. Store parts: `tools/` (Bun, 81 MB), `store/` (shards),
`work/` (installs), `<id>/` (the merged index). Each app was run COLD (an
empty store; Bun pre-placed unless noted), then FOUND (the same store), then
WARM (the store with its merged index deleted: shards reused, merge only).

| bundle                                                                 | size    | packages revealed / built / failed | cold build (run wall)                  | found run wall                   | warm build | store after                    | peak tree RSS |
| ---------------------------------------------------------------------- | ------- | ---------------------------------- | -------------------------------------- | -------------------------------- | ---------- | ------------------------------ | ------------- |
| doctoc (bun-dev)                                                       | 386 KB  | 0 (inert)                          | —                                      | —                                | —          | 0                              | —             |
| better-svg (no Bun: fetched)                                           | 2.6 MB  | 5 (+1 dropped) / 2 / 3             | 4 s (14 s)                             | 7 s                              | 3 s        | 108 MB                         | 1.6 GB        |
| novel-engine                                                           | 7.0 MB  | 2 / 2 / 0                          | 0.4 s (29 s)                           | 27 s                             | 0 s        | 85 MB                          | 3.4 GB        |
| array-index-inlay                                                      | 13.2 MB | 15 (+4 dropped) / 13 / 2           | 3 min (255 s)                          | 60 s                             | 5 s        | 585 MB                         | 5.0 GB        |
| Claude Code 2.1.216 (no index)                                         | 10 MB   | 0 valid (2 dropped) — inert        | — (run as off: 6,319 s whole stub run) | —                                | —          | 0                              | 30.0 GB       |
| Claude Code 2.1.216, prebuilt full index FOUND (`/work/library-index`) |         | —                                  | no build                               | 4,947 s (82 min, whole stub run) | —          | 84 GB store (cb64c1f5 = 43 GB) | 29.6 GB       |

Per built version: 1.5 / 12.5 / 39 MB of disk and 0.5 / 2 / 14 s at the
builder's 4 jobs. The estimate uses 12–40 MB and 2–15 s per version to build,
plus 10 s fixed (`VERSION_BYTES`, `VERSION_SECONDS`, `BUILD_FIXED_SECONDS`).
For scale: the prebuilt full index is 65,839 versions, 43 GB (0.65 MB per
version for the merged index alone), and took a 6.4 h shard phase on the
exp152/161 farm. A run-time build never comes near that, because it indexes
only what the bundle reveals.

The Claude Code found-path row was measured once with the pre-flip binary
(`--pre-flip`, identical find-and-load code; the only change is the one
"using" line).

## Tests and goldens — what changed and why

- **Config** (`library_config_test.rs`, `config_test.rs`,
  `runtime_index_test.rs`, `discover_verb_test.rs`, `config_file_test.rs`):
  - the default is names + auto;
  - `--library-matching off` is the opt-out and never reaches the machinery;
  - names needs no unlock, and the unlock key is refused;
  - without the run-time build, an index is required;
  - `humanify.example.toml` is regenerated from the registry.
- **CLI** (`tests/pipeline_stages.rs`):
  - a default run on a bundle no index can serve writes EXACTLY the opt-out's
    bytes and says so (`a_default_run_no_index_can_serve_runs_as_off_and_says_so`);
  - the opt-out and an explicit index never touch the store;
  - a found index prints the one line and never the notice;
  - "names refused until an eval" is gone.
- **Machinery** (`cache_test.rs`, `runtime_index_test.rs`):
  - names that are not npm package names are dropped, each with its reason;
  - a run-time index covers only the packages it was built from;
  - the estimate scales with the versions to build;
  - the notice states what will be built, the estimate, where, and the opt-out.
  - `build.json` gains `dropped` and `failed`.
  - The run-time build now records names tables (`names: true`), because names
    mode restores identifiers from them.
- **The library score** (`experiments/096-library-matching-score/run.py`,
  `discover_verb_test.rs`) pins `library.matching=report`, the mode every
  card before exp171 was scored in. A names request reads the index's module
  answers, and a scoring index without them would fail.
- **e2e** (`scripts/e2e.ts`): every launch runs the DEFAULT mode against a
  committed 108 KB store (`test/e2e/library-index/`, one index built from
  ms, the one real npm package the fixtures bundle). This keeps e2e meaningful
  for the new default, because every Bun fixture runs names mode end to end on
  the FOUND path: index lookup, vendor placement (`vendor/ms/`) and
  restoration. The esbuild and plain fixtures run the inert path. It is
  hermetic: a launch that prints the build notice FAILS the stage. The
  alternative, pinning e2e to `off`, would have left the shipped default
  untested. Result: 17 pairs pass and 1 known gap fails as declared
  (`bun-text-assets`, defect 2 above). Six fixtures found the index, three
  ran plain-input inert and nine ran esbuild inert. The legacy goldens
  (`test/golden/legacy-default/`) are unchanged: those fixtures are not Bun
  bundles, so they run inert, and the inert path writes off's bytes.
- **Eval launch goldens** (`run-launch.golden.txt`,
  `run-launch.seeded-base.golden.txt`): re-recorded. They differ ONLY by one
  `[library] index = "<pairs.json libraryIndex>"` section in each of the 10
  launches' configs; every argv is unchanged. `launch_parity_test.rs` asserts
  the key equals the eval's index, then compares the rest against the
  captured launches as before.
- **Parity goldens** (`test/parity/`): untouched, and every replay passes.
  They call stage functions with explicit inputs, not the CLI default.

## The eval harness

- `eval score` / `eval neutrality` resolve the NAMING MODE once
  (`namingModeOf`, scripts/eval.ts). The mode is `names` unless
  `--pipeline-set library.matching=off|report`; report writes off's bytes,
  so it counts as off. A names launch is handed `--library-index` (default:
  pairs.json `libraryIndex`). `library.index` is the harness's own key and is
  refused in `--pipeline-set`.
- run.sh records `namingMode` and `libraryIndex` in the label's
  `pipeline.json`.
- `labelNamingModes` (run-manifest.ts) reads `pipeline.json` first, then each
  manifest's `resolved.library.matching`. A label with no record is `off`:
  every label before exp171 was default-mode, and the names labels
  exp157–169 carry their own record.
- The leaderboard REFUSES labels in different naming modes unless
  `--force-mixed`, printing a `naming:` line next to the `base:` line.
  Summaries print `NAMING: <mode>`.
- `noise-bands.json` holds the names-mode bands under `byMode.names`. The
  top-level bands (off mode, `ref-scratch-0f338ffa-r1..r3`) are untouched. The
  leaderboard reads the bands of the mode it compares, and `eval bands` writes
  into the repeats' own mode slot. `self-hop-reference.json` holds the
  names-mode cold range under `byMode.names`. A mode with no range is "not
  judged", never judged against another mode's range.
- Neutrality across the flip: a baseline before exp171 defaults to off, so
  compare with `--pipeline-set library.matching=off`.

## The new reference

(filled in below when the three repeats finish)

## What this does not look at (rule 8)

- The run-time build's numbers come from three bun-dev apps (17 built
  versions). Claude Code contributes nothing to them, because it reveals no
  package. The estimate is a range, not a promise.
- The seed scan is unchanged (exp154: banners plus inlined `package.json`).
  What it offers is mostly tooling banners, and no package appears in Claude
  Code's text. Better seeds (the discovery's own evidence; the full index as
  a downloadable artifact) are the obvious next lever. Until then, names mode
  pays off only with a prebuilt index.
- The store is resolved against the OUTPUT dir (exp154's choice). Two output
  directories therefore build twice. Andrew's plan said "relative to the
  working directory". Not changed here; flagged.
