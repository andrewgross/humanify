#!/usr/bin/env bash
# exp181: score each kind corpus against its kind index (the exp096 library verb; exp150's two-pass layers recipe:
# the first score is refused for the missing copies layer, layers2.sh builds it and scores), and bun-dev against the
# Bun index built the same way. Under the heavy lock.
#   experiments/181-index-all-bundlers/scripts/score.sh <commit>
set -uo pipefail
C=$1
E=$(cd "$(dirname "$0")/../.." && pwd)
D=/work/exp181-data
cd "$E/.."
for k in esbuild webpack rollup; do
  npm run eval -- library exp181-$k --index $D/ix-$k --rule structural --corpus kind-$k --commit "$C" || true
  "$E/150-bun-testbed/scripts/layers2.sh" kind-$k exp181-$k
done
npm run eval -- library exp181-bun --index $D/ix-bun --rule structural --corpus bun-dev --commit "$C"
echo "SCORE DONE"
