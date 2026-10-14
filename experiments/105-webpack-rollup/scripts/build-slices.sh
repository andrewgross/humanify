#!/usr/bin/env bash
# exp105: build an index of the webpack and rollup recipe slices with a given humanify-index binary, under the
# campaign's index-build lock (waits for it; never two index builds at once). The package list is split into 4
# parts built in parallel (4 jobs each) pinned to CPUS (<= 30 cores), then one merge run over the whole list
# (reuses every shard). Same shape as experiments/097-index-recipes/scripts/build-index.sh, with the policy, the
# date and the recipes as arguments.
# usage: build-slices.sh <data-dir> <name> <cpu-list> <humanify-index> <packages.txt> <as-of> <policy> -- <flags>
#   <data-dir> holds store/, work/, logs/, parts/ and ix/<name>; the registry, recipes and their scripts are flags
#   (e.g. --registry R --recipe webpack:target=node,format=cjs,minify --webpack W --rollup S).
set -euo pipefail
[ $# -ge 8 ] && [ "$8" = "--" ] || { sed -n 2,9p "$0"; exit 2; }
DATA=$1; NAME=$2; CPUS=$3; IX=$4; LIST=$5; ASOF=$6; POLICY=$7; shift 8
common=(--as-of "$ASOF" --store "$DATA/store" --work "$DATA/work" --policy "$POLICY" --offline "$@")
mkdir -p "$DATA/ix" "$DATA/logs" "$DATA/parts"
echo "waiting for /work/index-build.lock $(date -u +%FT%TZ)"
exec 9>/work/index-build.lock
flock 9
echo "lock held $(date -u +%FT%TZ)"
start=$(date +%s)
"$IX" build --packages "$LIST" "${common[@]}" --dry-run > "$DATA/logs/plan-$NAME.txt" 2>&1 \
  || { tail -3 "$DATA/logs/plan-$NAME.txt"; exit 1; }
echo "plan: $(grep -c '^  build shard' "$DATA/logs/plan-$NAME.txt") shards to build, $(grep -c '^  reuse shard' "$DATA/logs/plan-$NAME.txt") reused"
rm -rf "$DATA/parts/$NAME-"*
split -n r/4 -d "$LIST" "$DATA/parts/$NAME-"
pids=()
for p in "$DATA/parts/$NAME-"0?; do
  taskset -c "$CPUS" "$IX" build --packages "$p" "${common[@]}" --out "$p.ix" --jobs 4 > "$p.log" 2>&1 &
  pids+=($!)
done
for p in "${pids[@]}"; do wait "$p" || echo "part $p exited $?"; done
parts_done=$(date +%s)
taskset -c "$CPUS" "$IX" build --packages "$LIST" "${common[@]}" --out "$DATA/ix/$NAME" --jobs 4 \
  > "$DATA/logs/merge-$NAME.log" 2>&1
end=$(date +%s)
{ echo "shard seconds $(( parts_done - start ))"; echo "merge seconds $(( end - parts_done ))"; echo "build seconds $(( end - start ))"; } \
  >> "$DATA/logs/merge-$NAME.log"
tail -n 4 "$DATA/logs/merge-$NAME.log"
du -sb "$DATA/ix/$NAME" | cut -f1 | sed 's/^/index bytes /'
echo "lock released $(date -u +%FT%TZ)"
