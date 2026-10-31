# exp174 — names mode requires a real library index

STATUS: BUILT (2026-10-31), branch `exp174-index-required`, rebased on main
`dfc0bd12e` (exp171 + exp173 merged). Andrew's decisions (2026-10-31):

1. Names mode REQUIRES a real library index, built or pointed to. The silent
   fallback to `off` goes away. The build-from-what-the-bundle-reveals path
   stays, behind its own explicit flag.
2. The index store lives in the WORKING directory.

## Totals first

- **No index → a loud failure before any work.** Names (the default) or
  report with no `--library-index` uses an index FOUND in the store. An empty
  store is refused at the parse-time check (`Config::library_index_problem`,
  before the input is read: exit 1, nothing created). A store whose indexes
  have no slice for the bundle is refused after detection, before unpack
  writes anything, naming each index and why it was not used. ONE text
  (`runtime_index::no_index_error`):

  ```
  Error: library names (--library-matching names, the default) needs a library index, and none was found.
    Looked in: <cwd>/.humanify/library-index (the library-index store: library.index_store_dir, resolved against the working directory) — it does not exist, so it holds no index (no <id>/manifest.json in it).
    Build one with the offline index builder (cargo build --release -p humanify-index; it fetches from the npm registry and bundles with Bun):
      humanify-index build --packages packages.txt --as-of <YYYY-MM-DD> --names \
        --registry <dir> --store <dir> --work <dir> --out <index-dir> \
        --recipe bun:target=bun,format=cjs,minify,production \
        --recipe bun:target=node,format=cjs,minify,production --bun <bun binary>
      (packages.txt: one npm package per line, `name` or `name@version`)
    Point the run at it: --library-index <index-dir>, or move it into the store as <store>/<sha256 of its manifest.json>/ (--library-index-store <dir> names another store).
    Or build one now from the npm packages this bundle itself reveals (library banners, inlined package.json; installs from the npm registry): --library-index-from-bundle
    Or run without library names: --library-matching off
  ```

- **The flag: `--library-index-from-bundle`** (key
  `library.index_from_bundle`, parsed at the one parse point with the other
  library flags). It takes exp171's path unchanged: notice, estimate,
  tolerant build, and the inert-as-`off` notice for a bundle that reveals
  nothing. It is refused with `--library-matching off` and with
  `--library-index` (it would be silently ignored). `library.index_build`
  (off | auto) is deleted; a file that sets it is refused as an unknown key.
- **The store: `<cwd>/.humanify/library-index/`** (`library.index_store_dir`
  / `--library-index-store`; relative resolves against the working
  directory, absolute is used as given). exp154 put only the store under the
  output dir's `.humanify/`: its `<id>/` indexes, `store/` shards,
  `registry/`, `work/` and `tools/` all live INSIDE the store directory, so
  they all moved with it. Everything else under the output's `.humanify/`
  (`library-report.json`, `vendor-humanified.js`, `humanified.js`, the split
  ledger, stage hashes, placement stats) is per-run output and stays.

## Tests

- Config (`library_config_test.rs`): the default searches the cwd store and
  does not build; an absolute store dir is used as given; names/report with
  an empty store is a problem naming every way out; a store holding an index
  is not; the flag opts in (and an empty store is then no problem); the flag
  is refused with off and with an index; `index_build` is refused.
- Machinery (`runtime_index_test.rs`): by default a store with nothing for
  the bundle answers `Failed` (Bun and esbuild alike), creating nothing; the
  inert test runs with the flag.
- CLI (`tests/pipeline_stages.rs`), red on the old binary, green now:
  `names_with_no_index_fails_loudly_before_any_work` (exit 1, no output dir,
  nothing in the cwd: no store, no fetch),
  `a_store_with_nothing_for_this_bundle_fails_loudly_naming_why`,
  `an_index_in_the_working_directory_store_is_found_and_used` (byte-identical
  to the explicit-index run; nothing of the store in the output dir),
  `a_pointed_store_is_used` (relative and absolute),
  `the_from_bundle_flag_takes_the_run_time_build_path`,
  `the_opt_out_and_an_explicit_index_never_touch_the_store`. Launches that
  make no library choice in these tests, `ask_trace.rs`,
  `replay_determinism.rs` and `provider_errors.rs` now say
  `library.matching = "off"`: they ran names INERT before, which writes off's
  bytes.
- e2e: still points every launch at the committed store
  (`test/e2e/library-index/`) explicitly. Bun fixtures run the default (6
  pairs, index FOUND); the 12 non-Bun pairs (3 plain, 9 esbuild) run
  `--library-matching off`, because the store has Bun slices only and names
  now refuses them instead of running inert. A launch that fails for a
  missing index fails the stage on its exit code; one that prints the build
  notice or the inert line fails it too. 18 pairs pass, 0 known gaps. **No
  golden changed**: the legacy goldens are non-Bun fixtures, whose inert run
  wrote off's bytes.
- Help golden and `humanify.example.toml` regenerated (the new flag/key, the
  store's location, the stale `--library-matching` help text fixed).

## Neutrality (the eval always passes `--library-index`)

RESULTS_PLACEHOLDER

## Proposed CLAUDE.md addition (for Andrew)

After "`--library-matching off` is the opt-out." in the names-mode paragraph:

> Names mode REQUIRES a library index (exp174): with no `--library-index` a
> run uses one found in `<cwd>/.humanify/library-index` and fails before any
> work when there is none; `--library-index-from-bundle` opts into the
> run-time build from what the bundle reveals.
