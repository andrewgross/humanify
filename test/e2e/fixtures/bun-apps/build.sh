#!/usr/bin/env bash
# Regenerates the two bun-apps e2e fixtures (see README.md):
#
#   bun-plain/  a Bun app with only static imports — NO lazy-init module
#   bun-mixed/  the same app with two modules loaded lazily through
#               require(), the other four eager
#   bun-lazy/   the same app with ALL six modules loaded lazily, a
#               two-statement entry (the shape the module markers describe)
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

for app in plain mixed lazy; do
  for ver in 1.0.0 1.1.0; do
    sha256sum "$fixtures/bun-$app/build/v$ver/build/index.js" | sed "s|$fixtures/||"
  done
done
