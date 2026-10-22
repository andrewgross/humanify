#!/usr/bin/env bash
# exp136: k2-all-r5-origin-prod with the `static` recipe flag on both Bun recipes and the esbuild recipe (a third app
# entry, a static `import * as` of every subpath), everything else exactly as exp135's build. Seeded from exp135: the
# store and work dir are hard-link copies; the req/dyn builds are reused (a build's stamp is the recipe without
# `static`), each static slice builds its `sta` entry and re-fingerprints all three; rollup/webpack shards are reused;
# the literal shards' key names every recipe, so they rebuild. Extra builder flags after the 3rd arg.
# usage: build-static.sh <data-dir> <humanify-index> [name [extra builder flags...]]
set -euo pipefail
[ $# -ge 2 ] || { sed -n 2,8p "$0"; exit 2; }
D=$1; IX=$2; NAME=${3:-k2-all-r5-origin-prod-static}; shift 3 2>/dev/null || shift $#
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" /work/exp101-data/roots-base.txt /work/exp108-data/pins-k2.txt 400 -- \
  --sponge-regions origin "$@" \
  --recipe bun:target=bun,format=cjs,minify,production,static \
  --recipe bun:target=node,format=cjs,minify,production,static \
  --recipe esbuild:target=node,format=cjs,minify,static --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify \
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun \
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild \
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
