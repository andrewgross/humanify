#!/usr/bin/env bash
# Re-measure the exp038 ceiling + gate breakdown against the CURRENT shipped
# behaviour (Lever B v2, trees under /tmp/eval-work/leverb/<n>-on-v2), not the
# superseded v1 trees that sit in /tmp/eval-work/leverb-sweep/<v>-on.
#
# 24 GB box: run this ALONE, never beside a pipeline run.
set -uo pipefail
REPO=/Users/andrewgross/Development/humanify
E38=$REPO/experiments/038-dependency-aware-reorder
HEAP="--max-old-space-size=10240"
for p in 86:85-\>86 119:118-\>119 198:197-\>198 216:215-\>216; do
  n=${p%%:*}; label=${p##*:}
  REB=/tmp/eval-work/leverb-sweep/2.1.$n-rebased
  V2=/tmp/eval-work/leverb/$n-on-v2
  [ -d "$V2/src" ] || { echo "MISSING $V2"; continue; }
  echo "########## $label — Lever B v2 (current shipped) ##########"
  NODE_OPTIONS="$HEAP" npx tsx "$E38/reorder-ceiling.ts" "$REB/src" "$V2/src" "$label" 2>&1 | tail -12
  NODE_OPTIONS="$HEAP" npx tsx "$E38/align-trace.ts" "$REB" "$V2" 2>&1 | tail -16
done
