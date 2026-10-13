#!/usr/bin/env bash
# Build one index with humanify-index (lane "indexv2"): both slices, a version policy, the list split into 4 parts
# built in parallel (4 jobs each, all pinned to CPUS) over one shard store, then one merge run over the whole list.
# usage: build-index.sh <list.txt> <name> <cpu-list>
#   env: DATA (store/work/registry/ix root), POLICY, SLICES, IX, PINNED (a --pinned file, split like the list)
set -u
DATA=${DATA:-/work/exp095-index-v2-data/selABC}
LIST=$1; NAME=$2; CPUS=$3
POLICY=${POLICY:-as-of}; SLICES=${SLICES:-bun,esbuild}
BUN=/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun
ESB=/work/exp095-library-fingerprints/lab/tools/node_modules/@esbuild/linux-x64/bin/esbuild
IX=${IX:-/work/exp095-index-v2-data/bin/humanify-index-v2}
ASOF=2026-10-11
mkdir -p "$DATA/ix" "$DATA/logs" "$DATA/parts"
common=(--as-of $ASOF --registry "$DATA/registry" --store "$DATA/store" --work "$DATA/work" --slices "$SLICES"
  --bun "$BUN" --esbuild "$ESB" --policy "$POLICY")
pin_all=(); [ -n "${PINNED:-}" ] && pin_all=(--pinned "$PINNED")
start=$(date +%s)
# fetch every packument once, serially (the parallel parts then run --offline)
"$IX" build --packages "$LIST" "${pin_all[@]}" "${common[@]}" --dry-run > "$DATA/logs/plan-$NAME.txt" 2>&1 || { tail -3 "$DATA/logs/plan-$NAME.txt"; exit 1; }
split -n r/4 -d "$LIST" "$DATA/parts/$NAME-"
[ -n "${PINNED:-}" ] && split -n r/4 -d "$PINNED" "$DATA/parts/$NAME-pins-"
pids=()
for p in "$DATA/parts/$NAME-"0?; do
  pin=(); [ -n "${PINNED:-}" ] && pin=(--pinned "$DATA/parts/$NAME-pins-${p##*-}")
  GOMAXPROCS=2 taskset -c "$CPUS" "$IX" build --packages "$p" "${pin[@]}" "${common[@]}" --out "$p.ix" --jobs 4 --offline \
    > "$p.log" 2>&1 &
  pids+=($!)
done
for p in "${pids[@]}"; do wait "$p"; done
GOMAXPROCS=2 taskset -c "$CPUS" "$IX" build --packages "$LIST" "${pin_all[@]}" "${common[@]}" --out "$DATA/ix/$NAME" --jobs 4 --offline \
  > "$DATA/logs/merge-$NAME.log" 2>&1
echo "build seconds $(( $(date +%s) - start ))" >> "$DATA/logs/merge-$NAME.log"
tail -n 2 "$DATA/logs/merge-$NAME.log"
