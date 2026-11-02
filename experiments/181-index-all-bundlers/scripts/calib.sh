#!/usr/bin/env bash
# exp181: what one built version costs per recipe bundler (the run-time build notice's estimate,
# humanify_index::cache::version_cost) — the same 30 versions (a seeded sample of the kind pins) built ONCE per kind,
# each from an empty shard store and work dir as a first run-time build would (installs included), at the run-time
# build's 4 jobs, by the pinned tool. Prints wall seconds and bytes (store + index + work) per built version.
#   experiments/181-index-all-bundlers/scripts/calib.sh <humanify-index binary>
set -euo pipefail
BIN=$1
D=/work/exp181-data
C=$D/calib
T=$D/store/tools
mkdir -p $C
python3 -I -c "import random; p=[l for l in open('$D/pins.txt').read().split() if l]; random.Random(181).shuffle(p); open('$C/pins30.txt','w').write(''.join(x+'\n' for x in sorted(p[:30])))"
: > $C/empty.txt
for k in bun esbuild webpack rollup; do
  case $k in
    bun) R="bun:target=node,format=cjs,minify,production" ;;
    esbuild) R="esbuild:target=node,format=cjs,minify,production" ;;
    webpack) R="webpack:target=node,format=cjs,minify" ;;
    rollup) R="rollup:target=node,format=cjs,minify" ;;
  esac
  rm -rf $C/$k && mkdir -p $C/$k
  s=$(date +%s)
  "$BIN" build --packages $C/empty.txt --pinned $C/pins30.txt --as-of 2026-10-25 --registry $D/store/registry \
    --store $C/$k/store --work $C/$k/work --out $C/$k/ix --names --jobs 4 --recipe "$R" \
    --bun $T/bun-1.4.0/node_modules/.bin/bun --esbuild $T/esbuild-0.28.2/node_modules/.bin/esbuild \
    --webpack $T/webpack-5.111.1/webpack-build.cjs --rollup $T/rollup-4.64.5/rollup-build.mjs > $C/$k.log 2>&1 || true
  e=$(date +%s)
  b=$(du -sb $C/$k | cut -f1)
  echo "CALIB $k seconds=$((e - s)) bytes=$b versions=30 $(grep -o 'index id [0-9a-f]*' $C/$k.log | head -1)"
done
