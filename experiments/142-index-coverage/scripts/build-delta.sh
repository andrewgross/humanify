#!/usr/bin/env bash
# exp142: the as-of DELTA of the recommended index k2-all-r5-origin-prod: every version its own version policies
# pick at the new as-of that was published after the old one (delta_plan.py), built with exactly exp135's recipes
# (both Bun slices `production`, --sponge-regions origin) into a hard-link copy of exp135's store, then ONE merge of
# the old index's every version + the delta (all passed as --pinned, so no policy re-pick drops an old version).
# usage: build-delta.sh <data-dir> <humanify-index> <as-of> <all-pins.txt> <delta-pins.txt> [name]
#   <data-dir> needs registry/ (a FRESH snapshot: fetch_registry.py), store/ (cp -al of exp135's), work/ (empty)
set -euo pipefail
[ $# -ge 5 ] || { sed -n 2,7p "$0"; exit 2; }
D=$1; IX=$2; ASOF=$3; PINS=$4; DELTA=$5; NAME=${6:-k2-all-r5-origin-prod-asof${ASOF//-/}}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
: > "$D/empty.txt"
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" "$D/empty.txt" "$PINS" 100 --as-of "$ASOF" --build-pins "$DELTA" -- \
  --sponge-regions origin \
  --recipe bun:target=bun,format=cjs,minify,production --recipe bun:target=node,format=cjs,minify,production \
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify \
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun \
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild \
  --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
