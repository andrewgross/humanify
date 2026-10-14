#!/usr/bin/env bash
# exp109: build the campaign's default index, c2-all = exp101's c2-l12m4 package set (step 4's roots + pins and
# exp101's 1,811 new roots at year-last12+major4) across ALL FIVE recipe slices (Bun target bun, Bun target node,
# esbuild, webpack, rollup), with main's builder (/3 format, streaming merge). The shard store is a hard-linked
# union of exp101's store (three Bun/esbuild slices) and exp105's (webpack/rollup at the as-of policy), so only the
# missing shards build: webpack/rollup for every version exp105 did not build, and every literals shard (its key
# holds the whole recipe set, so a five-slice index re-reads literals from the builds already in the work dir).
# Parts build in parallel under the campaign's index-build lock (<= 30 cores), then one merge-only run.
# usage: build-c2-all.sh <flags.sh> <name> <cpu-list> <parts>
#   flags.sh sets D (data dir: store/, work/, logs/, parts/, ix/), IX (humanify-index), COMMON (build flags),
#   LIST and PINS (exp101's roots-base.txt and pins-c2-l12m4.txt).
set -euo pipefail
[ $# -eq 4 ] || { sed -n 2,11p "$0"; exit 2; }
# shellcheck disable=SC1090
source "$1"; NAME=$2; CPUS=$3; NP=$4
[ -e "$D/ix/$NAME" ] && { echo "REFUSED: $D/ix/$NAME exists (never overwrite an index)"; exit 1; }
mkdir -p "$D/ix" "$D/logs" "$D/parts"
exec 9>/work/index-build.lock
echo "waiting for /work/index-build.lock $(date -u +%FT%TZ)"
flock 9
start=$(date +%s)
echo "lock held $(date -u +%FT%TZ)"
"$IX" build --packages "$LIST" --pinned "$PINS" "${COMMON[@]}" --dry-run > "$D/logs/plan-$NAME.txt" 2>&1 \
  || { tail -3 "$D/logs/plan-$NAME.txt"; exit 1; }
echo "plan: $(grep -c '^  build shard' "$D/logs/plan-$NAME.txt") shards to build, $(grep -c '^  reuse shard' "$D/logs/plan-$NAME.txt") reused"
find "$D/parts" -maxdepth 1 -name "$NAME-*" -exec rm -rf {} +
split -n "r/$NP" -d "$LIST" "$D/parts/$NAME-"
split -n "r/$NP" -d "$PINS" "$D/parts/$NAME-pins-"
pids=()
for p in "$D/parts/$NAME-"0?; do
  taskset -c "$CPUS" "$IX" build --packages "$p" --pinned "$D/parts/$NAME-pins-${p##*-}" "${COMMON[@]}" \
    --out "$p.ix" --jobs 4 > "$p.log" 2>&1 &
  pids+=($!)
done
for p in "${pids[@]}"; do wait "$p" || echo "part $p exited $?"; done
parts_done=$(date +%s)
taskset -c "$CPUS" "$IX" build --packages "$LIST" --pinned "$PINS" "${COMMON[@]}" --merge-only --out "$D/ix/$NAME" \
  > "$D/logs/merge-$NAME.log" 2>&1
end=$(date +%s)
{ echo "shard seconds $(( parts_done - start ))"; echo "merge seconds $(( end - parts_done ))"; echo "build seconds $(( end - start ))"; } \
  >> "$D/logs/merge-$NAME.log"
tail -n 4 "$D/logs/merge-$NAME.log"
du -sb "$D/ix/$NAME" | cut -f1 | sed 's/^/index bytes /'
echo "BUILD DONE $NAME $(date -u +%FT%TZ)"
