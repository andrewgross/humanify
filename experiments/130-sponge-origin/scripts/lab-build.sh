#!/usr/bin/env bash
# exp130: lab-build.sh <data-dir> <humanify-index> <name> <drop|origin>  (exp124's lab-build.sh + --sponge-regions)
set -euo pipefail
D=$1; IX=$2; NAME=$3; MODE=$4
REPO=/work/exp130-sponge-origin
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" /work/exp101-data/roots-base.txt /work/exp108-data/pins-k2.txt 400 -- \
  --sponge-regions "$MODE" \
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify \
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify \
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun \
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild \
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
