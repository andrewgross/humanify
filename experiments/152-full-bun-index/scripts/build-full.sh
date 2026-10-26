#!/usr/bin/env bash
# exp152: the FULL Bun index with call edges (format /4): every pin of the installed /3 index plus the as-of delta
# (delta_plan.py), both Bun slices `production`, --sponge-regions origin, Bun 1.4.0, into a FRESH store and work dir.
# Short-lived chunk processes (build_chunks.py) in waves under /work/heavy.lock (released between waves so another
# lane's eval can run), the whole build under /work/index-build.lock, at most 30 cores (34-63), then one --merge-only.
# npm and Bun run through timing shims (make_shims.sh / timed.sh) so step_times.py can split the wall time.
# usage: build-full.sh <data-dir> <humanify-index> <as-of YYYY-MM-DD> <pins.txt> <bun> [name]
#   <data-dir> needs registry/ (fetch_registry.py: every pinned package + bun), store/ and work/ absent or empty
set -euo pipefail
[ $# -ge 5 ] || { sed -n 2,8p "$0"; exit 2; }
D=$1; IX=$2; ASOF=$3; PINS=$4; BUN=$5; NAME=${6:-full-bun-edges-asof${ASOF//-/}}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
"$HERE/make_shims.sh" "$D/shims" "$(command -v npm)" "$BUN"
: > "$D/empty.txt"
export PATH="$D/shims:$PATH" EXP152_TIMING_LOG="$D/logs/timing.log"
exec flock /work/index-build.lock python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" "$D/empty.txt" "$PINS" 60 --as-of "$ASOF" \
  --wave-lock /work/heavy.lock --progress "$D/progress.txt" -- \
  --sponge-regions origin \
  --recipe bun:target=bun,format=cjs,minify,production --recipe bun:target=node,format=cjs,minify,production \
  --bun "$D/shims/bun"
