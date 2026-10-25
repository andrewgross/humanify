#!/usr/bin/env bash
# exp151 timing: warm legs (work dir already filled) alternating main/edges twice, fresh stores; then one cold edges build.
set -uo pipefail
D=/work/exp151-data
BUN=/work/tools/bun-1.4.0/node_modules/.bin/bun
run() { # label binary store out work
  local t0=$(date +%s%N)
  taskset -c 34-63 "$2" build --packages $D/empty.txt --pinned $D/pins.txt --as-of 2026-10-15 \
    --registry $D/registry --store "$3" --work "$5" --out "$4" \
    --recipe 'bun:target=node,format=cjs,minify,production' --recipe 'bun:target=bun,format=cjs,minify,production' \
    --bun $BUN --jobs 4 --offline --sponge-regions origin > $D/t-$1.log 2>&1
  local rc=$?
  local t1=$(date +%s%N)
  echo "$1 rc=$rc wall_ms=$(( (t1 - t0) / 1000000 ))" >> $D/times4.txt
}
M=/work/exp151-main/target/release/humanify-index
E=/work/exp151-edges/target/release/humanify-index
for i in 1 2; do
  rm -rf $D/t-store-m$i $D/t-store-e$i
  run main-warm-$i $M $D/t-store-m$i $D/t-ix-m$i $D/work
  run edges-warm-$i $E $D/t-store-e$i $D/t-ix-e$i $D/work
done
run edges-cold $E $D/t-store-ecold $D/t-ix-ecold $D/work-cold2
echo done >> $D/times4.txt
