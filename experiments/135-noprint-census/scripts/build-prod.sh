#!/usr/bin/env bash
# exp135: k2-all-r5-origin with both Bun slices built under the `production` recipe flag (process.env.NODE_ENV
# defined as "production", as app bundles are built), everything else exactly as exp130's full build. Seeded from
# exp130: the store and work dir are hard-link copies (a production slice rebuilds into its own copy of each build
# dir: build() removes the old one first; the literal shards' key names every recipe, so they rebuild too).
# usage: build-prod.sh <data-dir> <humanify-index> [name]
#   <data-dir> needs registry/ (a copy of exp130's; build_chunks passes --offline), store/shards/ (cp -al of exp130's), work/ (cp -al)
set -euo pipefail
[ $# -ge 2 ] || { sed -n 2,7p "$0"; exit 2; }
D=$1; IX=$2; NAME=${3:-k2-all-r5-origin-prod}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" /work/exp101-data/roots-base.txt /work/exp108-data/pins-k2.txt 400 -- \
  --sponge-regions origin \
  --recipe bun:target=bun,format=cjs,minify,production --recipe bun:target=node,format=cjs,minify,production \
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify \
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun \
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild \
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
