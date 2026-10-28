#!/usr/bin/env bash
# exp159 census build A (feasibility, existing machinery, no code change):
# one build of the sample pins under BOTH the index's minified recipe and
# the same recipe without `minify` — the unminified bundle is what the
# names table's alignment anchor (the canonical serialize's slot ordinals)
# would be read from. Fresh store and work dir; npm cache is warm.
#
# Usage: build_census.sh <store> <work> <out> [extra humanify-index args...]
#   $1 store dir   $2 work dir   $3 index out dir
# Env: BIN (humanify-index), PINS (pin file, default pins-all.txt)
set -uo pipefail
D=/work/exp159-data
REPO=/work/exp159-restore
BIN=${BIN:-$REPO/target/release/humanify-index}
BUN=/work/tools/bun-1.4.0/node_modules/.bin/bun
PINS=${PINS:-$D/pins-all.txt}
STORE=$1; WORK=$2; OUT=$3
shift 3

cd $D
(
  flock 9
  (
    flock 8
    taskset -c 34-63 "$BIN" build --packages $D/empty.txt --pinned "$PINS" \
      --as-of 2026-10-15 --registry /work/exp151-data/registry \
      --store "$STORE" --work "$WORK" --out "$OUT" \
      --recipe 'bun:target=node,format=cjs,minify,production' \
      --recipe 'bun:target=node,format=cjs,production' \
      --bun $BUN --jobs 4 --offline --sponge-regions origin "$@"
  ) 8>/work/heavy.lock
) 9>/work/index-build.lock
rc=$?
echo "build_census rc=$rc"
exit $rc
