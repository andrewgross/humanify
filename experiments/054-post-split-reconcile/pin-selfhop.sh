#!/usr/bin/env bash
#
# exp054 — the self-hop idempotence invariant, DRAW-PINNED, for both legs.
#
#   experiments/054-post-split-reconcile/pin-selfhop.sh [workdir]
#
# 049 read self-hop 16 -> 326 from a cold A/B and it was entirely draw variance.
# So this pins the draws through the cache and self-hops off EACH leg's own
# output; the only variable left is the pass.
#
# It also compares the split TREE, not just the bundle. 049's version compared
# `humanified.js` alone, which for this pass would be blind: the post-split
# reconcile never touches the bundle, so a bundle-only invariant cannot fail
# however wrong the pass is.
#
# PREDICTION, stated before the run: on a self-hop the prior tree IS the fresh
# tree, so every per-file diff is empty, so the pass proposes nothing and is
# inert. Both legs should read the same. A non-zero tree diff on the ON leg that
# the OFF leg does not have is the pass failing to be a fixed point.
set -uo pipefail

WORK="${1:-/work}"
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
cd "$REPO"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
CACHE="${SELFHOP_CACHE:-$WORK/exp054-cache}"

TAG="${EXP054_TAG:-exp054}"
TO="${EXP054_SELFHOP_TO:-2.1.216}"
# The kill switch under test — parameterised alongside `pinned-ab.sh`, so a
# second experiment reuses this gate rather than forking it.
FLAG="${PINNED_AB_FLAG:-HUMANIFY_NO_POST_SPLIT_RECONCILE}"
INPUTS="${EVAL_INPUTS_BASE:-$(jq -r .inputsBase "$CFG")}"
INPUT="$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"
ENDPOINT="${EVAL_ENDPOINT:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")

selfhop() {
  local LEG="$1" KILL="$2" BASE OUT
  BASE="$WORK/$LEG"
  OUT="$WORK/$LEG-selfhop"
  [[ -f "$BASE/.humanify/humanified.js" ]] || {
    echo "FATAL: no base bundle for $LEG" >&2
    return 1
  }
  echo "=== self-hop off $LEG ($FLAG='$KILL') ==="
  rm -rf "$OUT"
  export "$FLAG=$KILL"
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    --llm-cache "$CACHE" \
    --prior-version "$BASE/.humanify/humanified.js" \
    > "$WORK/$LEG-selfhop.stdout" 2>&1
  unset "$FLAG"
  if [[ ! -f "$OUT/.humanify/humanified.js" ]]; then
    echo "  PIPELINE FAILED (see $WORK/$LEG-selfhop.stdout)"
    return 1
  fi
  local BUNDLE=0 TREE=0 MOVES=0
  if ! cmp -s "$BASE/.humanify/humanified.js" "$OUT/.humanify/humanified.js"; then
    BUNDLE=$(diff "$BASE/.humanify/humanified.js" "$OUT/.humanify/humanified.js" | grep -cE '^[<>]')
    MOVES=$(diff "$BASE/.humanify/humanified.js" "$OUT/.humanify/humanified.js" | grep -cE '^[0-9,]+[ad]' || true)
  fi
  TREE=$(diff -r "$BASE/src" "$OUT/src" 2>/dev/null | grep -cE '^[<>]' || true)
  echo "  $LEG self-hop: bundle $BUNDLE diff lines ($MOVES move hunks), src/ tree $TREE diff lines"
  echo "ROW|$LEG|$BUNDLE|$MOVES|$TREE"
}

BEFORE=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "cache entries before: $BEFORE"
# OFF first, so any prompt it has to draw is on disk when ON asks for it.
selfhop "$TAG-off-$TO" 1
MID=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "  cache written by the OFF leg: $((MID - BEFORE))"
selfhop "$TAG-on-$TO" ""
AFTER=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "  cache written by the ON leg:  $((AFTER - MID))   <-- near zero is what makes the two comparable"
echo "PINNED SELF-HOP COMPLETE"
