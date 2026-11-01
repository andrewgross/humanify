# exp175 — the bundle's kind decides the library index

STATUS: BUILT and PROVEN (2026-11-01), branch `exp175-index-by-bundle-kind`,
on main `5a48564bd` (exp174 merged). Gate 13/13 in the worktree and from a
clean checkout of `c95043550`. Warm neutrality NEUTRAL in names and off mode.
Not merged.

Andrew's decision (2026-11-01): "use our discovery of the bundle type to
inform how we build our index, and there is some level of 'no existing index
found for <bundle type>, skipping/building' depending on config/flags."

The problem it fixes: exp174 made names mode (the default) REQUIRE a library
index, and every index that exists is Bun-built, so every esbuild, webpack or
plain-script bundle FAILED in default mode.

## Totals first

- **The kind is decided in ONE place.** `recipe::index_kind` (humanify-core)
  names the bundle's packing tool: for a recipe bundler exactly
  `RecipeBundler::name` (`bun`, `esbuild`, `webpack`, `rollup`), which is the
  key index slices are already recorded and searched under. No second key.
  A bundler no recipe exists for still has a name (`browserify`, `parcel`,
  `unrecognised` when detection names no bundler). `DetectedRecipe` carries
  it as `kind`. `detect_recipe` now runs ONCE per run (`pipeline_body`), and
  both the store search and the library load use that result. Before, they
  each called it.
- **Found → used**, as before. The line now names the kind:
  `Library index: using <id> for bun bundles from <store> (found; nothing to build)`.
- **None for the kind, default config → one line, library name matching
  skipped, no failure.** The run writes exactly off mode's bytes (test:
  byte-identical to `--library-matching off`).
- **None for the kind, `--library-index-from-bundle` → build line, then the
  existing notice and build.** This is for Bun bundles only, because the
  run-time build can only fetch and run Bun. Any other kind with the flag is
  skipped with its own line. So is a Bun bundle that reveals no npm package.
- **An explicit `--library-index` with no slice for the kind still FAILS** a
  names run, and the error now names the kind. (report mode keeps its
  warning: report changes no bytes.)
- **A skipped run cannot pass for a names run.** The stats file gets a new
  `libraryMatching` block: `matching`, `outcome` = used / built / skipped /
  off, `kind`, `indexFrom` = explicit / store / run-time build, `indexId`, and
  `why` (the skip line). The library report gets the same block as `outcome`.
  The eval's run manifest copies it as `library`, and a new manifest warning,
  `library-skipped`, fires on a skip. `invariants.ts` REFUSES the pair's card
  (exit 3) when the scored leg's block says skipped, or when the rebuilt
  base's stdout has the skip line. That is the same path as the
  lost-LLM-answers refusal. The eval always passes the prebuilt index
  explicitly, so it never skips by construction; the refusal is a backstop.
- **Deleted:** `runtime_index::no_index_error` (exp174's multi-line refusal),
  the parse-time empty-store refusal in `Config::library_index_problem`,
  `JobAnswer::Inert` and exp171's "Library names: off for this run" notices
  (`no_package_notice`), `LibraryLoad.inert`, and the report's `inert` field.
  `JobAnswer::Skipped` and the skip lines replace them. Config is unchanged:
  no new flag or key.

## What a user sees (real binary, `target/debug/humanify`, dead endpoint)

Bun bundle, index for Bun bundles in the store (e2e `bun-bundle`, committed store):

```
Library index: using 0242134fb663 for bun bundles from /work/exp175-index-by-bundle-kind/test/e2e/library-index (found; nothing to build)
```

esbuild bundle, no index (e2e `esbuild-cjs`, empty working directory), exit 0:

```
no library index for esbuild bundles in <cwd>/.humanify/library-index — skipping library name matching (build one with `humanify-index build --names --recipe esbuild:target=node,format=cjs,minify,production …`, or point to one with --library-index <dir>)
```

esbuild bundle with `--library-index-from-bundle`, exit 0:

```
no library index for esbuild bundles in <cwd>/.humanify/library-index, and --library-index-from-bundle builds Bun indexes only — skipping library name matching (build one with `humanify-index build --names --recipe esbuild:target=node,format=cjs,minify,production …`, or point to one with --library-index <dir>)
```

esbuild bundle with an explicit Bun-only index, exit 1:

```
Error: --library-matching names cannot run: the index passed with --library-index has no slice for esbuild bundles (it was named, so this is not skipped) — library index 0242134fb663347fed2bd6740f5ca4842d8c0af617e57f9957a8dccc5bdf50aa has no slice for recipe esbuild:target=?,format=?,minify (has: bun:target=bun,format=cjs,minify,production, bun:target=node,format=cjs,minify,production)
```

A Bun bundle with no index: the skip line also offers
`--library-index-from-bundle`. With that flag, a Bun bundle that reveals
packages prints
`no library index for bun bundles — building one from this bundle into <store>`
and then exp171's NOTICE. This was not run end to end: it needs the npm
registry. The text is pinned by `the_building_line_names_the_kind_and_the_store`.

The `--recipe` in the skip line is the recipe THIS bundle needs
(`DetectedRecipe::needed_recipe`): the detected bundler, target and format,
with node / cjs where the bundle records none.

## Tests (red first, then green)

- core `recipe_test.rs`: `the_index_kind_is_the_bundles_packing_tool` and
  `the_needed_recipe_is_read_off_detection`.
- cli `runtime_index_test.rs`, through the real job → channel → join:
  - `by_default_no_index_for_the_bundles_kind_skips_with_one_line`: Bun and
    esbuild. One line, the kind, the store, the builder `--recipe <kind>:`,
    `--library-index <dir>`, and the from-bundle offer only for Bun. Nothing
    is created.
  - `a_bundle_of_a_kind_no_index_can_have_is_skipped_saying_so`: unrecognised,
    no recipe offered.
  - `from_bundle_skips_what_it_cannot_build_and_says_why`: esbuild, and Bun
    with no package.
  - `the_building_line_names_the_kind_and_the_store`.
- cli `library_config_test.rs`:
  `names_with_an_empty_store_and_no_index_is_no_problem_before_work` replaces
  exp174's refusal test.
- CLI `tests/pipeline_stages.rs`. Before the change: 4 red, 1 green (the
  explicit-unfit guard already held; its message test was added with the new
  wording).
  - `no_index_for_the_bundles_kind_skips_library_names_with_one_line`: exit 0,
    one line, nothing created in the cwd. Stats record
    `{matching: names, outcome: skipped, kind: bun, why: <the line>}`. Output
    byte-identical to `--library-matching off`, which records `off`. report
    does the same, and its report says skipped.
  - `a_store_holding_only_another_kinds_index_is_skipped`: the trail names the
    Bun index and "no slice for recipe".
  - `an_explicit_index_that_does_not_fit_the_bundles_kind_fails_loudly`.
  - `an_index_in_the_working_directory_store_is_found_and_used`: records
    `used` / `store` / `bun` / the id. The explicit leg records `explicit`.
  - `the_from_bundle_flag_takes_the_run_time_build_path`: the new line.
- harness `experiments/lib/library-skip.test.ts`: refuses a skipped scored
  leg and a skipped rebuilt base. A used index and off are not refused. A
  missing block is UNKNOWN (null) and not refused. The CLI exits 3.
  `run-manifest.test.ts`: the `library-skipped` warning.
- e2e (`scripts/e2e.ts`): EVERY fixture now runs the default. Bun fixtures
  must print the found line. Every non-Bun fixture must print the skip line
  naming its own kind, because the committed `ms` store has Bun slices only.
  The `librarySettings` off-switch is deleted.
- Stats schema golden (`test/parity/wpb4-stats-schema.json`) gains the
  optional `libraryMatching`. Help golden and `humanify.example.toml` are
  regenerated.

## Proof

| check                                                  | result                                                                      |
| ------------------------------------------------------ | --------------------------------------------------------------------------- |
| `npm run check`, worktree                              | ALL 13 STAGES PASSED (e2e: 19 pairs, 7 Bun found, 12 non-Bun skip line)     |
| `npm run check`, clean checkout `c95043550`            | ALL 13 STAGES PASSED                                                        |
| neutrality names (both legs `library.index=cb64c1f5…`) | NEUTRAL: 0 files / 0 lines, exit 0 / 0, cache candidate +0, baseline **+0** |
| neutrality off (`--pipeline-set library.matching=off`) | NEUTRAL: 0 files / 0 lines, exit 0 / 0, cache candidate +0, baseline **+0** |

Both neutrality runs were WARM, on 2.1.85→2.1.86 against main `5a48564bd`,
run the way exp174 ran them:
`npm run eval -- neutrality 5a48564bd 2.1.85:2.1.86 --priors /work/exp050-cold --cache /work/neutrality-cache`,
with `--pipeline-set library.matching=off` added for off mode. Claude Code
is a Bun bundle, and the eval passes its index explicitly, so its output
cannot change, and it did not.

## What this does NOT do

- **No run-time build for esbuild (or webpack, rollup).** The offline builder
  (`humanify-index build`) builds all four recipe kinds, and the skip line
  points to it. The run-time build still fetches and runs only Bun. Adding
  esbuild means fetching an esbuild binary and choosing its recipes. More
  importantly, names-mode precision on esbuild bundles with an esbuild index
  has never been measured against the 99.8% bar. That is a decision for
  Andrew, not a side effect of this lane.
- The kind is the packing tool alone. Within a kind, the store search still
  picks the slice by target (Bun `--target bun` vs node), exactly as before.
  A Bun bundle whose target has no slice in the store is skipped like a
  missing kind, and its trail names each index and why it did not serve.

## Proposed CLAUDE.md text (for Andrew)

Replace exp174's proposed sentence after "`--library-matching off` is the
opt-out." with:

> Which index a names run needs is the bundle's KIND, its detected packing
> tool (exp175): with no `--library-index` the run uses an index of that kind
> found in `<cwd>/.humanify/library-index`, and when there is none it prints
> one `no library index for <kind> bundles … — skipping library name
matching` line and writes off mode's bytes (recorded as `libraryMatching` > `outcome: skipped`, which the eval refuses to score). An explicit
> `--library-index` that does not fit the kind fails;
> `--library-index-from-bundle` builds one at run time, for Bun bundles only.
