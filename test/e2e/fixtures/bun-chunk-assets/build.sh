#!/usr/bin/env bash
# Regenerates the bun-chunk-assets e2e fixture (see README.md): the
# bun-apps `assets` app (../bun-apps/source/gen.mjs) as a COMPILED,
# code-split ESM executable —
#
#   bun build src/main.js --compile --splitting --format esm --minify
#
# — unpacked with bun-decompile, and the executable's own stdout recorded
# beside each build (`build/v<ver>/compiled.stdout`): the unpacked layout
# reads its embedded text files through Bun's runtime
# (`import.meta.require`), which Node does not have, so the input's
# behaviour is what the compiled app printed.
#
# Needs Bun 1.4.1 (BUN, default `bun` on PATH; refuses any other: the
# embedded-file shapes are Bun 1.4's) and bun-decompile (BUN_DECOMPILE,
# default `bun-decompile` on PATH). Prints each file's sha256; README.md
# records them.
#
#   BUN=/path/to/bun-1.4.1 BUN_DECOMPILE=/path/to/bun-decompile \
#     bash test/e2e/fixtures/bun-chunk-assets/build.sh
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
  src="$scratch/assets-v$ver"
  node "$fixtures/bun-apps/source/gen.mjs" assets "$ver" "$src"
  (cd "$src" && npm install --no-audit --no-fund --ignore-scripts --silent)
  (cd "$src" && "$bun_bin" build src/main.js --compile --splitting --format esm --minify \
    --outfile="$src/app" >/dev/null)
  out="$here/build/v$ver"
  rm -rf "$out/build"
  mkdir -p "$out"
  "$decompile" "$src/app" -o "$out/build" >/dev/null
  (cd "$scratch" && "$src/app" >"$out/compiled.stdout")
done

for ver in 1.0.0 1.1.0; do
  (cd "$here" && find "build/v$ver" -type f | LC_ALL=C sort | xargs sha256sum)
done
