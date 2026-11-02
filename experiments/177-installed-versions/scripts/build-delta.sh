#!/usr/bin/env bash
# exp177: the installed-versions DELTA on top of the full names index cb64c1f5 (exp161's build, exp152's pins).
# Builds shards ONLY for <delta.pins> into a hard-link copy of exp161's store (cp -al: the builder writes every
# shard with write-then-rename, so a link is never written through), then ONE --merge-only over exp152's pins +
# the delta, with exactly exp161's recipe flags (both Bun slices production, --sponge-regions origin, --names, the
# four pinyin refusals, --as-of 2026-10-25, the same registry snapshot, --offline) and --merge-jobs 30.
# The merge reads shards only; the work dir holds just the delta's installs.
# Bun 1.4.0 directly (exp152/161 called it through a timing shim whose script is gone; the shard key records the
# bundler VERSION, not its path).
# Run as ONE heavy job, holding both locks for the whole build (no --wave-lock: with other lanes always queued on
# heavy.lock a wave yields before any chunk starts, and the wave driver crashed on exactly that — fixed in
# build_chunks.py, but yielding every wave would starve the build):
#   flock /work/index-build.lock flock /work/heavy.lock build-delta.sh <args>
# usage: build-delta.sh <data-dir> <humanify-index> <delta.pins> [name]
#   <data-dir> must hold store/ (cp -al of /work/exp161-data/store) and registry -> /work/exp161-data/registry
set -euo pipefail
[ $# -ge 3 ] || { sed -n 2,12p "$0"; exit 2; }
D=$1; IX=$2; DELTA=$3; NAME=${4:-full-bun-names-iv-asof20261025}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
BASE_PINS=/work/exp152-data/pins.txt
: > "$D/empty.txt"
LC_ALL=C sort -u "$BASE_PINS" "$DELTA" > "$D/pins-all.txt"
R="memory-hog per Andrew 2026-10-29: unpopular, OOMs workers at 64GB"
exec python3 -I "$REPO/experiments/108-app-corpus/scripts/build_chunks.py" \
  "$D" "$NAME" 34-63 7 "$IX" "$D/empty.txt" "$D/pins-all.txt" 60 --as-of 2026-10-25 \
  --build-pins "$DELTA" --progress "$D/progress.txt" -- \
  --sponge-regions origin --names --merge-jobs 30 \
  --refuse "pinyin@0.0.1:$R" --refuse "pinyin@2.11.2:$R" --refuse "pinyin@3.1.0:$R" --refuse "pinyin@4.0.0:$R" \
  --recipe bun:target=bun,format=cjs,minify,production --recipe bun:target=node,format=cjs,minify,production \
  --bun /work/tools/bun-1.4.0/node_modules/.bin/bun
