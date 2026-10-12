#!/usr/bin/env bash
# Build one selection-lab index with humanify-index (Bun slice; the default as-of policy: latest, previous patch,
# major-1, major-2). The list is split into 4 parts built in parallel (4 jobs each, pinned to CPUS), sharing the
# shard store; then one merge run over the whole list (every shard reused) writes $SEL/ix/<name>.
# usage: build-index.sh <list.txt> <name> <cpu-list, e.g. 24-39>
set -u
SEL=${SEL:-/work/exp095-library-fingerprints/selection}
LIST=$1; NAME=$2; CPUS=$3
BUN=/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun
IX=${IX:-/work/exp095-index-selection/target/release/humanify-index}
ASOF=2026-10-11
mkdir -p "$SEL/ix" "$SEL/logs" "$SEL/parts"
common=(--as-of $ASOF --registry "$SEL/registry" --store "$SEL/store" --work "$SEL/work" --slices bun --bun "$BUN")
# fetch every packument once, serially (the parallel parts then run --offline)
"$IX" build --packages "$LIST" "${common[@]}" --dry-run > "$SEL/logs/plan-$NAME.txt" 2>&1
split -n r/4 -d "$LIST" "$SEL/parts/$NAME-"
pids=()
for p in "$SEL/parts/$NAME-"0*; do
  GOMAXPROCS=2 taskset -c "$CPUS" "$IX" build --packages "$p" "${common[@]}" --out "$p.ix" --jobs 4 --offline \
    > "$p.log" 2>&1 &
  pids+=($!)
done
for p in "${pids[@]}"; do wait "$p"; done
"$IX" build --packages "$LIST" "${common[@]}" --out "$SEL/ix/$NAME" --jobs 1 --offline > "$SEL/logs/merge-$NAME.log" 2>&1
tail -n 1 "$SEL/logs/merge-$NAME.log"
