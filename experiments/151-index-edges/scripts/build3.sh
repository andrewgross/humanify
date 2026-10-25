#!/usr/bin/env bash
# exp151: three builds of the same 330 pins (Bun slices only): main cold (fills the work dir), edges warm, main warm.
set -uo pipefail
D=/work/exp151-data
BUN=/work/tools/bun-1.4.0/node_modules/.bin/bun
run() { # label binary store out
  local t0=$(date +%s.%N)
  taskset -c 34-63 "$2" build --packages $D/empty.txt --pinned $D/pins.txt --as-of 2026-10-15 \
    --registry $D/registry --store "$3" --work $D/work --out "$4" \
    --recipe 'bun:target=node,format=cjs,minify,production' --recipe 'bun:target=bun,format=cjs,minify,production' \
    --bun $BUN --jobs 4 --offline --sponge-regions origin > $D/$1.log 2>&1
  local rc=$?
  local t1=$(date +%s.%N)
  echo "$1 rc=$rc wall=$(echo "$t1 - $t0" | bc)" >> $D/times.txt
}
cd $D
run main-cold /work/exp151-main/target/release/humanify-index $D/store-main-cold $D/ix-main-cold
run edges-warm /work/exp151-edges/target/release/humanify-index $D/store-edges $D/ix-edges
run main-warm /work/exp151-main/target/release/humanify-index $D/store-main-warm $D/ix-main-warm
echo done >> $D/times.txt
