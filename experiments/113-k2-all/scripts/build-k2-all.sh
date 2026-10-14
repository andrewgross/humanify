#!/usr/bin/env bash
# exp113: build k2-all = exp108's k2-l12m4 package set (c2's roots + pins and exp108's new roots at year-last12+major4)
# across ALL FIVE recipe slices (Bun target bun, Bun target node, esbuild, webpack, rollup), with main's builder
# (mimalloc: flat memory) in exp108's chunk processes under the campaign lock (30 cores, cpus 34-63).
# The store is a hard-linked union of exp109's (exp101's three slices incl. k2's shards, exp105's and exp109's
# webpack/rollup, ~15.7k five-recipe literals) topped up from exp101/exp105; the work dir is seeded with
# seed-work.sh (installs and finished builds hard-linked), so only missing shards build.
# usage: build-k2-all.sh <data-dir> <humanify-index> [name]
set -euo pipefail
[ $# -ge 2 ] || { sed -n 2,8p "$0"; exit 2; }
D=$1; IX=$2; NAME=${3:-k2-all}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" /work/exp101-data/roots-base.txt /work/exp108-data/pins-k2.txt 400 -- \
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify \
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify \
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun \
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild \
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
