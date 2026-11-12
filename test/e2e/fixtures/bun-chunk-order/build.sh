#!/usr/bin/env bash
# Regenerates the bun-chunk-order e2e fixture (see README.md): the
# bun-apps `order` app (../bun-apps/source/gen.mjs) as a COMPILED,
# code-split ESM executable —
#
#   bun build src/main.js --compile --splitting --format esm --minify
#
# — unpacked with bun-decompile. The entry loads every module through
# import(), so Bun's runtime chunk is itself loaded lazily.
#
# Needs Bun 1.4.1 (BUN, default `bun` on PATH; refuses any other) and
# bun-decompile (BUN_DECOMPILE, default `bun-decompile` on PATH). Prints
# each file's sha256; README.md records them.
#
#   BUN=/path/to/bun-1.4.1 BUN_DECOMPILE=/path/to/bun-decompile \
#     bash test/e2e/fixtures/bun-chunk-order/build.sh
set -euo pipefail

BUN_VERSION=1.4.1
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixtures="$(dirname "$here")"
bun_bin="${BUN:-$(command -v bun || echo "$HOME/.bun/bin/bun")}"
got_bun="$("$bun_bin" --version)"
if [ "$got_bun" != "$BUN_VERSION" ]; then
  echo "need Bun $BUN_VERSION, found $got_bun ($bun_bin)" >&2
  exit 1
fi
decompile="${BUN_DECOMPILE:-bun-decompile}"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

for ver in 1.0.0 1.1.0; do
  src="$scratch/order-v$ver"
  node "$fixtures/bun-apps/source/gen.mjs" order "$ver" "$src"
  (cd "$src" && "$bun_bin" build src/main.js --compile --splitting --format esm --minify \
    --outfile="$src/app" >/dev/null)
  out="$here/build/v$ver/build"
  rm -rf "$out"
  mkdir -p "$(dirname "$out")"
  "$decompile" "$src/app" -o "$out" >/dev/null
done

for ver in 1.0.0 1.1.0; do
  (cd "$here" && find "build/v$ver" -type f | LC_ALL=C sort | xargs sha256sum)
done
