#!/usr/bin/env bash
# exp133: rebuild exp118's k2-all + dl25k with main's current builder (unit-rows/5+origin keys, slim maps), as an
# INCREMENT on k2-all-r5-origin. The store is a hard-linked union: exp130's full store (every k2-all shard at
# unit-rows/5+origin, reused as-is) overlaid on exp118's (only its LITERALS shards still match: their key has no
# unit-rows version; its old unit-rows/4 module shards are rebuilt). exp118's work dirs were pruned, so every dl25k
# version is installed AND built in this one work dir (exp124's lesson holds by construction). Same pins as exp118
# (pins-k2 + tail-dl25k), same bundler binaries.
# usage: build-dl25k-r5.sh <data-dir> <humanify-index> [name]
set -euo pipefail
[ $# -ge 2 ] || { sed -n 2,8p "$0"; exit 2; }
D=$1; IX=$2; NAME=${3:-k2-all-r5-origin-dl25k}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" /work/exp101-data/roots-base.txt /work/exp118-data/pins-dl25k.txt 400 -- \
  --sponge-regions origin \
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify \
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify \
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun \
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild \
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
