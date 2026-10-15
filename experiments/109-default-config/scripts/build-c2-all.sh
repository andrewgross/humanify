#!/usr/bin/env bash
# exp109: build the campaign's default index, c2-all = exp101's c2-l12m4 package set (step 4's roots + pins and
# exp101's 1,811 new roots at year-last12+major4) across ALL FIVE recipe slices (Bun target bun, Bun target node,
# esbuild, webpack, rollup), with main's builder (/3 format, streaming merge) in exp108's short-lived chunk
# processes (a build process leaks ~40 MB per version; experiments/108-app-corpus/scripts/build_chunks.py).
# The shard store is a hard-linked union of exp101's store (the three Bun/esbuild slices) and exp105's (webpack/rollup
# at the as-of policy), and the work dir is seeded from earlier builds (installs hard-linked, finished builds copied:
# seed-work.sh), so only the missing shards build: webpack/rollup for every version exp105 did not build, and every
# literals shard (its key holds the whole recipe set, so a five-slice index re-reads literals from the builds).
# usage: build-c2-all.sh <data-dir> <humanify-index> [name]
#   <data-dir> holds registry/ (exp101's), store/, work/; writes ix/<name> (default c2-all).
set -euo pipefail
[ $# -ge 2 ] || { sed -n 2,11p "$0"; exit 2; }
D=$1; IX=$2; NAME=${3:-c2-all}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 6 "$IX" /work/exp101-data/roots-base.txt /work/exp101-data/pins-c2-l12m4.txt 100 -- \
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify \
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify \
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun \
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild \
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
