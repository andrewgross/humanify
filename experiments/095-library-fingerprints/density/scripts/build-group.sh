#!/usr/bin/env bash
# One humanify-index process over one group of pins (density lab). All groups share one shard store and work tree.
# usage: build-group.sh <group.txt> <cpu-list>     (e.g. 44-47: the process and its bundlers stay on those cores)
set -u
DEN=${DEN:-/work/exp095-library-fingerprints/density}
G=$1; CPUS=$2; NAME=$(basename "$G" .txt)
ESB=/work/exp095-library-fingerprints/lab/tools/node_modules/@esbuild/linux-x64/bin/esbuild
BUN=/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun
IX=${IX:-/work/exp095-index-selection/target/release/humanify-index}
GOMAXPROCS=2 exec taskset -c "$CPUS" "$IX" build --packages "$G" --as-of 2026-10-11 --registry "$DEN/registry" \
  --store "$DEN/store" --work "$DEN/work" --out "$DEN/ix-$NAME" --slices bun,esbuild --bun "$BUN" --esbuild "$ESB" \
  --jobs 4 --offline > "$DEN/logs/build-$NAME.log" 2>&1
