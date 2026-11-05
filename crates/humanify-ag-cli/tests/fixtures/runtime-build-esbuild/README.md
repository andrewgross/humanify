# The run-time build's esbuild fixture (exp181)

`pipeline_stages.rs` `from_bundle_builds_an_esbuild_index_from_the_store_s_own_tool`
runs `--library-index-from-bundle` on `bundle.js` with no network: the store it
copies from `store/` already holds everything the build needs.

- `app/` — the source: an app that uses `ms` and prints its `package.json`
  version, so esbuild inlines `ms`'s package.json and the bundle REVEALS the
  package (`name: "ms", version: "2.1.3"`), which seeds the build.
- `bundle.js` — `esbuild 0.28.2 index.js --bundle --platform=node --format=cjs --minify`
  (after `npm install` in `app/`).
- `store/registry/` — the packument snapshots the plan reads (`ms`, and
  `esbuild`, which dates the bundler).
- `store/store/shards/` — the shards of ms@2.1.3 for the recipe this bundle
  needs, `esbuild:target=node,format=cjs,minify,production`, built by esbuild
  0.28.2 (the pinned tool, `humanify_ag_index::tools::pin`).
- `stub-esbuild.sh` — the test installs it as the store's esbuild
  (`tools/esbuild-0.28.2/node_modules/.bin/esbuild`; `node_modules/` is
  gitignored, so the test lays it down). It answers `--version` and refuses to
  build: the committed shard must serve.

Rebuild (when a shard or fingerprint version moves, the shard goes stale, the
build reaches the stub and the test fails — that is the signal): run the real
binary with `--library-index-from-bundle` on `bundle.js` in an empty working
directory, which fetches esbuild 0.28.2 into its store and builds the shard,
then copy that store's `registry/` and `store/` here.
