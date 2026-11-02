#!/usr/bin/env bash
# exp181: the kind indexes — every package version the 45 bun-dev apps install (kind_corpus.py pins), built with
# each recipe the run-time build would build for that kind's bundles, by the pinned tools in the measurement store
# (humanify_index::tools layout), with names (`--names`, as the run-time build). ONE build of every recipe shares
# the installs; then one merge-only index per kind from the same shard store.
#   experiments/181-index-all-bundlers/scripts/ix.sh <humanify-index binary>
set -euo pipefail
BIN=$1
D=/work/exp181-data
S=$D/store
T=$S/tools
: > $D/empty.txt
R_BUN_NODE="bun:target=node,format=cjs,minify,production"
R_BUN_BUN="bun:target=bun,format=cjs,minify,production"
R_ESB="esbuild:target=node,format=cjs,minify,production"
R_WP="webpack:target=node,format=cjs,minify"
R_RL="rollup:target=node,format=cjs,minify"
common=(--packages $D/empty.txt --pinned $D/pins.txt --as-of 2026-10-25 --registry $S/registry --store $S/store
  --work $S/work --names --bun $T/bun-1.4.0/node_modules/.bin/bun --esbuild $T/esbuild-0.28.2/node_modules/.bin/esbuild
  --webpack $T/webpack-5.111.1/webpack-build.cjs --rollup $T/rollup-4.64.5/rollup-build.mjs)
step=${2:-all}
if [ "$step" = all ] || [ "$step" = build ]; then
  "$BIN" build "${common[@]}" --jobs 4 --out $D/ix-all \
    --recipe "$R_BUN_NODE" --recipe "$R_BUN_BUN" --recipe "$R_ESB" --recipe "$R_WP" --recipe "$R_RL"
fi
if [ "$step" = all ] || [ "$step" = merge ]; then
  "$BIN" build "${common[@]}" --merge-only --out $D/ix-esbuild --recipe "$R_ESB"
  "$BIN" build "${common[@]}" --merge-only --out $D/ix-webpack --recipe "$R_WP"
  "$BIN" build "${common[@]}" --merge-only --out $D/ix-rollup --recipe "$R_RL"
  "$BIN" build "${common[@]}" --merge-only --out $D/ix-bun --recipe "$R_BUN_NODE" --recipe "$R_BUN_BUN"
fi
echo "IX DONE"
