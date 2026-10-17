#!/usr/bin/env bash
# exp131: FRESH builds (no cached build) of 60 versions: main's builder + main's recipe scripts vs this branch's
# builder + its recipe scripts (no sourcesContent). Run under: flock /work/index-build.lock
set -euo pipefail
D=/work/exp131-data; F=$D/fresh
common=(--sponge-regions origin
  --recipe bun:target=bun,format=cjs,minify --recipe bun:target=node,format=cjs,minify
  --recipe esbuild:target=node,format=cjs,minify --recipe webpack:target=node,format=cjs,minify
  --recipe rollup:target=node,format=cjs,minify
  --bun /work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun
  --esbuild /work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild)
python3 -I $D/sample/build_chunks.py $F/old fresh 48-63 4 $D/bin/humanify-index-main $F/packages.txt $F/pins.txt 15 -- \
  "${common[@]}" --webpack /work/exp105-data/tools/webpack-build.cjs --rollup /work/exp105-data/tools/rollup-build.mjs
python3 -I $D/sample/build_chunks.py $F/new fresh 48-63 4 $D/bin/humanify-index-slim $F/packages.txt $F/pins.txt 15 -- \
  "${common[@]}" --webpack $D/tools/webpack-build.cjs --rollup $D/tools/rollup-build.mjs
echo FRESH DONE
