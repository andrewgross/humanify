#!/usr/bin/env bash
# exp101: build one coverage index over a shared shard store (only missing shards build), under the campaign's
# build lock. Same shape as exp097's build-index.sh (parts in parallel, then one merge run that reuses every shard),
# with the part count as a parameter and the lock taken here.
# usage: build-index.sh <data-dir> <name> <cpu-list> <parts> <humanify-index> <packages.txt> <pins.txt> -- <recipe flags>
#   <data-dir> holds store/, registry/, work/; writes ix/<name> (refuses an existing one), logs/, parts/.
set -euo pipefail
[ $# -ge 8 ] && [ "$8" = "--" ] || { sed -n 2,6p "$0"; exit 2; }
DATA=$1; NAME=$2; CPUS=$3; NP=$4; IX=$5; LIST=$6; PINS=$7; shift 8
[ -e "$DATA/ix/$NAME" ] && { echo "REFUSED: $DATA/ix/$NAME exists (never overwrite an index)"; exit 1; }
ASOF=2026-10-11
common=(--as-of "$ASOF" --registry "$DATA/registry" --store "$DATA/store" --work "$DATA/work"
  --policy year-all+minor "$@")
mkdir -p "$DATA/ix" "$DATA/logs" "$DATA/parts"
exec 9>/work/index-build.lock
echo "waiting for /work/index-build.lock $(date -u +%FT%TZ)"
flock 9
start=$(date +%s)
echo "lock held $(date -u +%FT%TZ)"
"$IX" build --packages "$LIST" --pinned "$PINS" "${common[@]}" --offline --dry-run > "$DATA/logs/plan-$NAME.txt" 2>&1 \
  || { tail -3 "$DATA/logs/plan-$NAME.txt"; exit 1; }
echo "plan: $(grep -c '^  build shard' "$DATA/logs/plan-$NAME.txt") shards to build, $(grep -c '^  reuse shard' "$DATA/logs/plan-$NAME.txt") reused"
rm -rf "$DATA/parts/$NAME-"*
split -n "r/$NP" -d "$LIST" "$DATA/parts/$NAME-"
split -n "r/$NP" -d "$PINS" "$DATA/parts/$NAME-pins-"
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
echo "BUILD DONE $NAME"
