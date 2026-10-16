#!/usr/bin/env bash
# exp124 lab: rebuild the affected module shards (lab-store.sh unlinked them) with a lab binary keyed unit-rows/4,
# then merge the whole index. Same package set, recipes and bundler binaries as k2-all (build-k2-all.sh).
# usage: lab-build.sh <lab-data-dir> <humanify-index> <name> <drop|origin>
set -euo pipefail
[ $# -eq 4 ] || { sed -n 2,4p "$0"; exit 2; }
D=$1; IX=$2; NAME=$3; MODE=$4
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" /work/exp101-data/roots-base.txt /work/exp108-data/pins-k2.txt 400 -- \
  --sponge-regions "$MODE" \
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify \
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify \
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun \
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild \
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
