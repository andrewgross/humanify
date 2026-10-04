#!/usr/bin/env bash
# Regenerates the four non-Bun-CJS e2e fixtures from ONE shared source,
# this directory's source/v<ver>/: bun-bundle's app (24 local CJS deps, the real
# npm package ms@2.1.3, a CJS module requiring an ES module lazily, a
# dynamic import, a deterministic stdout report) plus one Node builtin
# import (`node:path`), which both bundlers keep EXTERNAL — so the ESM
# builds carry a real top-level `import` statement, as ESM apps do.
#
#   esbuild-minified/  esbuild 0.27.2  --bundle --minify --format=iife --platform=node
#   esbuild-cjs/       esbuild 0.27.2  --bundle --format=cjs --platform=node
#   esbuild-esm/       esbuild 0.27.2  --bundle --format=esm --platform=node
#   bun-esm-minified/  Bun 1.3.14      build --target=bun --format=esm --minify
#
# Tool versions are PINNED: esbuild through `npx esbuild@0.27.2`; Bun is
# whatever `bun` is on PATH and the script refuses anything but 1.3.14.
# Each version is built from a scratch copy, from INSIDE the version's
# directory: both bundlers bake source paths relative to the working
# directory into the output (esbuild's `__commonJS` keys, Bun's `// src/…`
# comments), so the bytes only reproduce from the same relative layout.
#
#   bash test/e2e/fixtures/node-app/build.sh
#
# Prints each build's sha256; the fixtures' READMEs record them.
set -euo pipefail

ESBUILD_VERSION=0.27.2
BUN_VERSION=1.3.14
FIXTURES=(esbuild-minified esbuild-cjs esbuild-esm bun-esm-minified)

app_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
here="$(dirname "$app_dir")"
bun_bin="${BUN:-$(command -v bun || echo "$HOME/.bun/bin/bun")}"
got_bun="$("$bun_bin" --version)"
if [ "$got_bun" != "$BUN_VERSION" ]; then
  echo "need Bun $BUN_VERSION, found $got_bun ($bun_bin)" >&2
  exit 1
fi

# Each build ships the module type it was built for, as a real app's
# package.json would: the repo root is `"type": "module"`, so without one
# Node would load the iife/cjs builds as ESM, where esbuild's require()
# shim throws ("Dynamic require of \"node:path\" is not supported").
module_type() {
  case "$1" in
  *-esm*) echo module ;;
  *) echo commonjs ;;
  esac
}

esbuild() {
  npx -y "esbuild@$ESBUILD_VERSION" src/main.js --bundle --platform=node \
    --log-level=warning "$@"
}

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

for ver in 1.0.0 1.1.0; do
  src="$scratch/v$ver"
  cp -r "$app_dir/source/v$ver" "$src"
  (cd "$src" && npm install --no-audit --no-fund --ignore-scripts --silent)
  for fixture in "${FIXTURES[@]}"; do
    mkdir -p "$here/$fixture/build/v$ver/build"
    echo "{ \"type\": \"$(module_type "$fixture")\" }" \
      >"$here/$fixture/build/v$ver/build/package.json"
  done
  out() { echo "$here/$1/build/v$ver/build/index.js"; }
  (
    cd "$src"
    esbuild --minify --format=iife --outfile="$(out esbuild-minified)"
    esbuild --format=cjs --outfile="$(out esbuild-cjs)"
    esbuild --format=esm --outfile="$(out esbuild-esm)"
    "$bun_bin" build src/main.js --target=bun --format=esm --minify \
      --outfile="$(out bun-esm-minified)" >/dev/null
  )
done

for fixture in "${FIXTURES[@]}"; do
  for ver in 1.0.0 1.1.0; do
    sha256sum "$here/$fixture/build/v$ver/build/index.js" |
      sed "s|$here/||"
  done
done
