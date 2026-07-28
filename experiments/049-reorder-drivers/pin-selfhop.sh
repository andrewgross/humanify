#!/usr/bin/env bash
#
# 049 — is the self-hop regression the CHANGE, or the draw?
#
#   experiments/049-reorder-drivers/pin-selfhop.sh [workdir]
#
# The cold gate showed self-hop 16 (old) -> 326 (new). Every hunk was a
# change-in-place — zero statements moved, so emit order is a fixed point and the
# 326 lines are naming wobble on minted leftovers (`P6 -> LRUCache`, `i -> idx`),
# the same class as the control's 16 (`G -> elementHeightState`). But the two legs
# ran on DIFFERENT bundles, because a cold A/B redraws every name, so that
# comparison cannot separate the change from the draw.
#
# This can. The draw-pinned legs `exp049-reg-{on,off}` were produced with the
# cache and have BYTE-IDENTICAL bundles (33,954,589 each) — the change is
# split-only — while their split trees and ledgers differ. Self-hopping off each,
# through the cache, holds the naming draws fixed and leaves the emitted layout as
# the only variable:
#
#   both self-hops ~equal  => the 326 was draw variance, the change is exonerated
#   ON markedly worse      => emit order really does perturb next-hop naming
#                             through the priorNames cascade, and that is a real
#                             cost that has to be weighed against the -60% reorder
#
# Rule 10 note: the cache is legitimate here for the same reason as the rest of
# 049's pinned measurements — the question is about a deterministic surface
# (layout), and pinning is what isolates it. No number from this run is a KPI.
set -uo pipefail

WORK="${1:-/work}"
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
cd "$REPO"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
CACHE="${SELFHOP_CACHE:-$WORK/llm-cache}"

TO=2.1.216
INPUTS="${EVAL_INPUTS_BASE:-$(jq -r .inputsBase "$CFG")}"
INPUT="$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"
ENDPOINT="${EVAL_ENDPOINT:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")

selfhop() {
  local LEG="$1"
  local BASE="$WORK/$LEG"
  local OUT="$WORK/$LEG-selfhop"
  [[ -f "$BASE/.humanify/humanified.js" ]] || {
    echo "FATAL: no base bundle for $LEG" >&2
    return 1
  }
  echo "=== self-hop off $LEG ==="
  rm -rf "$OUT"
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    --llm-cache "$CACHE" \
    --prior-version "$BASE/.humanify/humanified.js" \
    > "$WORK/$LEG-selfhop.stdout" 2>&1
  if [[ ! -f "$OUT/.humanify/humanified.js" ]]; then
    echo "  PIPELINE FAILED (see $WORK/$LEG-selfhop.stdout)"
    return 1
  fi
  local D=0
  if ! cmp -s "$BASE/.humanify/humanified.js" "$OUT/.humanify/humanified.js"; then
    D=$(diff "$BASE/.humanify/humanified.js" "$OUT/.humanify/humanified.js" | grep -cE '^[<>]')
  fi
  # A statement that MOVED shows up as an insertion/deletion hunk; a renamed one
  # is a change-in-place. That distinction is the whole question.
  local MOVES
  MOVES=$(diff "$BASE/.humanify/humanified.js" "$OUT/.humanify/humanified.js" 2>/dev/null \
    | grep -cE '^[0-9,]+[ad]' || true)
  echo "  $LEG self-hop: $D diff lines, $MOVES insertion/deletion hunk(s)"
}

BEFORE=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "cache entries before: $BEFORE"

# OFF first, so any prompt it has to draw is on disk when ON asks for it.
selfhop exp049-reg-off
MID=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "  cache written by OFF leg: $((MID - BEFORE))"
selfhop exp049-reg-on
AFTER=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "  cache written by ON leg:  $((AFTER - MID))"

echo
echo "A near-zero write count on the SECOND leg is what makes the two comparable;"
echo "a large one means its prompts differed, which is itself the finding."
echo "PINNED SELF-HOP COMPLETE"
