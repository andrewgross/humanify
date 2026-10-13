#!/usr/bin/env bash
# exp097: build one index with a given humanify-index binary over a shared shard store (only missing shards build).
# The package list and pins are split into 4 parts built in parallel (4 jobs each) pinned to CPUS (24 cores =
# the lane's cap), then one merge run over the whole list (reuses every shard; parses nothing).
# usage: build-index.sh <data-dir> <work-dir> <name> <cpu-list> <humanify-index binary> <packages.txt> <pins.txt> -- <recipe flags>
#   <data-dir> holds store/ and registry/ (may be a symlink); writes ix/<name>, logs/, parts/.
#   <work-dir> is the install/build tree, passed VERBATIM: a build's sourcemap paths are relative to the out dir as the
#   bundler saw it (through any symlink), so reusing index v2's builds needs v2's own spelling,
#   /work/exp095-index-v2-data/final/work (a first step-1 build through another symlink attributed no unit: README).
#   The recipe flags are the binary's own (step 1: --slices bun,esbuild --bun B --esbuild E; step 2+: --recipe ...).
set -euo pipefail
[ $# -ge 8 ] && [ "$8" = "--" ] || { sed -n 2,11p "$0"; exit 2; }
DATA=$1; WORK=$2; NAME=$3; CPUS=$4; IX=$5; LIST=$6; PINS=$7; shift 8
ASOF=2026-10-11   # index v2's date: the same plan, so only the changed shard kinds rebuild
common=(--as-of "$ASOF" --registry "$DATA/registry" --store "$DATA/store" --work "$WORK"
  --policy year-all+minor "$@")
mkdir -p "$DATA/ix" "$DATA/logs" "$DATA/parts"
start=$(date +%s)
"$IX" build --packages "$LIST" --pinned "$PINS" "${common[@]}" --offline --dry-run > "$DATA/logs/plan-$NAME.txt" 2>&1 \
  || { tail -3 "$DATA/logs/plan-$NAME.txt"; exit 1; }
echo "plan: $(grep -c '^  build shard' "$DATA/logs/plan-$NAME.txt") shards to build, $(grep -c '^  reuse shard' "$DATA/logs/plan-$NAME.txt") reused"
rm -rf "$DATA/parts/$NAME-"*
split -n r/4 -d "$LIST" "$DATA/parts/$NAME-"
split -n r/4 -d "$PINS" "$DATA/parts/$NAME-pins-"
pids=()
for p in "$DATA/parts/$NAME-"0?; do
  taskset -c "$CPUS" "$IX" build --packages "$p" --pinned "$DATA/parts/$NAME-pins-${p##*-}" "${common[@]}" \
    --out "$p.ix" --jobs 4 --offline > "$p.log" 2>&1 &
  pids+=($!)
done
for p in "${pids[@]}"; do wait "$p" || echo "part $p exited $?"; done
parts_done=$(date +%s)
taskset -c "$CPUS" "$IX" build --packages "$LIST" --pinned "$PINS" "${common[@]}" --out "$DATA/ix/$NAME" --jobs 4 \
  --offline > "$DATA/logs/merge-$NAME.log" 2>&1
end=$(date +%s)
{ echo "shard seconds $(( parts_done - start ))"; echo "merge seconds $(( end - parts_done ))"; echo "build seconds $(( end - start ))"; } \
  >> "$DATA/logs/merge-$NAME.log"
tail -n 4 "$DATA/logs/merge-$NAME.log"
du -sb "$DATA/ix/$NAME" | cut -f1 | sed 's/^/index bytes /'
