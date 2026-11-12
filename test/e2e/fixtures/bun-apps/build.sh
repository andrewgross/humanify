#!/usr/bin/env bash
# Regenerates the two bun-apps e2e fixtures (see README.md):
#
#   bun-plain/  a Bun app with only static imports — NO lazy-init module
#   bun-mixed/  the same app with two modules loaded lazily through
#               require(), the other four eager
#   bun-lazy/   the same app with ALL six modules loaded lazily, a
#               two-statement entry (the shape the module markers describe)
#   bun-chunks/ the same app as a COMPILED, code-split ESM executable
#               (`bun build --compile --splitting --format esm --minify`),
#               unpacked with bun-decompile: an entry that imports
#               `/$bunfs/root/<chunk>.js` siblings — Claude Code's layout
#               from 2.1.242 (exp216). Four modules load through import().
#
# both built with `bun build src/main.js --target=bun --format=cjs`
# (unminified) — Bun's CommonJS output, the `// @bun @bun-cjs` wrapper.
# The sources are written by source/gen.mjs (deterministic) into a
# scratch directory, `npm install` pins ms@2.1.3, and each version is
# built from INSIDE its directory (Bun bakes `// src/…` comments relative
# to the working directory). Bun is whatever `bun` is on PATH; the script
# refuses anything but 1.3.14.
#
#   bash test/e2e/fixtures/bun-apps/build.sh
#
# bun-chunks also needs bun-decompile (BUN_DECOMPILE, default
# `bun-decompile` on PATH) to unpack the compiled executable.
#
# Prints each build's sha256; the fixtures' READMEs record them.
set -euo pipefail

BUN_VERSION=1.3.14
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixtures="$(dirname "$here")"
bun_bin="${BUN:-$(command -v bun || echo "$HOME/.bun/bin/bun")}"
got_bun="$("$bun_bin" --version)"
if [ "$got_bun" != "$BUN_VERSION" ]; then
  echo "need Bun $BUN_VERSION, found $got_bun ($bun_bin)" >&2
  exit 1
fi

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

for app in plain mixed lazy; do
  for ver in 1.0.0 1.1.0; do
    src="$scratch/$app-v$ver"
    node "$here/source/gen.mjs" "$app" "$ver" "$src"
    (cd "$src" && npm install --no-audit --no-fund --ignore-scripts --silent)
    out="$fixtures/bun-$app/build/v$ver/build/index.js"
    mkdir -p "$(dirname "$out")"
    (cd "$src" && "$bun_bin" build src/main.js --target=bun --format=cjs \
      --outfile="$out" >/dev/null)
  done
done

decompile="${BUN_DECOMPILE:-bun-decompile}"
for ver in 1.0.0 1.1.0; do
  src="$scratch/chunks-v$ver"
  node "$here/source/gen.mjs" chunks "$ver" "$src"
  (cd "$src" && npm install --no-audit --no-fund --ignore-scripts --silent)
  (cd "$src" && "$bun_bin" build src/main.js --compile --splitting --format esm --minify \
    --outfile="$src/app" >/dev/null)
  out="$fixtures/bun-chunks/build/v$ver/build"
  rm -rf "$out"
  "$decompile" "$src/app" -o "$out" >/dev/null
done

for app in plain mixed lazy; do
  for ver in 1.0.0 1.1.0; do
    sha256sum "$fixtures/bun-$app/build/v$ver/build/index.js" | sed "s|$fixtures/||"
  done
done
for ver in 1.0.0 1.1.0; do
  (cd "$fixtures" && sha256sum bun-chunks/build/v$ver/build/*.js)
done
