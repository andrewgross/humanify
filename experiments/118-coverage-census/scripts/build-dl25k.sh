#!/usr/bin/env bash
# exp118: build k2-all + dl25k = k2-all's package set (exp101's roots-base + exp108's pins-k2) PLUS every package in
# npm's last-month download top-25,000 (packages.ecosyste.ms ranking, 2026-10-15) that k2-all does not index and that
# is not @types/*, at the year-last3+major1 density (tail_pins.py), in all five slices. An INCREMENT: the store is a
# hard-linked copy of exp113's (every k2-all shard is reused; only the new versions, plus exp113's known-failing
# retries, build). Main's builder code is unchanged since f568e5bc (exp113's binary is used).
# usage: build-dl25k.sh <data-dir> <humanify-index> <pins.txt> [name]
set -euo pipefail
[ $# -ge 3 ] || { sed -n 2,7p "$0"; exit 2; }
D=$1; IX=$2; PINS=$3; NAME=${4:-k2-all-dl25k}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" /work/exp101-data/roots-base.txt "$PINS" 400 -- \
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify \
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify \
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun \
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild \
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
