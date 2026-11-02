# The e2e library-index store

Names mode is the binary's default (exp171): with no `--library-index`, a run
uses an index of the bundle's KIND (its packing tool, read off detection) that
it finds in its store (`library.index_store_dir`, default
`.humanify/library-index` under the working directory), and when there is none
prints one line and skips library name matching (exp175); only
`--library-index-from-bundle` builds one at run time. The e2e stage
(`scripts/e2e.ts`) points every launch at this committed store explicitly and
runs every fixture in the default mode: every Bun fixture takes the FOUND path
(no fetch, no build: a launch that prints the run-time build's notice fails the
stage), so the default mode runs end to end — index lookup, vendor placement,
restoration — on the release binary. Its slices are Bun's only, so every
non-Bun fixture must print the skip line naming its own kind.

`0242134fb663347fed2bd6740f5ca4842d8c0af617e57f9957a8dccc5bdf50aa/` is one merged index (its directory name is its
`manifest.json`'s sha256, the store's convention): `ms`, the one real npm
package the fixtures bundle (`bun-bundle` holds ms@2.1.3), the four versions
the `as-of` policy picks at 2026-10-25, both Bun 1.4.0 slices
(`bun:target={bun,node},format=cjs,minify,production`), with names tables
(`--names`). 108 KB.

Rebuild it (when the index format or a fingerprint version moves, the run
refuses it as stale and the e2e stage fails — that is the signal):

```bash
echo "ms@2.1.3" > packages.txt
humanify-index build --packages packages.txt --as-of 2026-10-25 \
  --registry registry --store store --work work --out out \
  --recipe bun:target=bun,format=cjs,minify,production \
  --recipe bun:target=node,format=cjs,minify,production \
  --bun <bun 1.4.0> --names
# then replace <id>/ with out/, renamed to sha256(out/manifest.json)
```
