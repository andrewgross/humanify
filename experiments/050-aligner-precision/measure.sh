#!/usr/bin/env bash
#
# 050 — the EXACT effect of keying alignment on (hash, name), draws pinned.
#
#   experiments/050-aligner-precision/measure.sh [workdir]
#
# WHY THIS NEEDS ITS OWN REBASE, unlike 049's probe. The benefit only exists when
# the PRIOR ledger carries `emitNames`, and every prior on disk predates the
# field — so measuring against one would read exactly zero and prove nothing.
# This rebases 2.1.215 with the CURRENT tree first, so the prior has names, then
# hops 2.1.216 off it twice: once keying on them, once with
# HUMANIFY_NO_NAME_ALIGN=1 ignoring them. Both legs then share one prior and the
# keying is the only variable.
#
# Cache is deliberate and legitimate here (rule 10): emission alignment is a
# deterministic surface downstream of every prompt, so pinning the draws isolates
# it. The diagnostic that it worked is the SECOND leg's write count.
set -uo pipefail

WORK="${1:-/work}"
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
cd "$REPO"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
CACHE="${MEASURE_CACHE:-$WORK/llm-cache}"
RESULTS="$REPO/experiments/034-eval-harness/results"

FROM=2.1.215
TO=2.1.216
INPUTS="${EVAL_INPUTS_BASE:-$(jq -r .inputsBase "$CFG")}"
PRIORS="${EVAL_PRIORS_BASE:-$(jq -r .priorsBase "$CFG")}"
ENDPOINT="${EVAL_ENDPOINT:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG"); APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG"); CONC=$(jq -r .llm.concurrency "$CFG")

run() {
  local OUT="$1"
  local PRIOR="$2"
  local LOG="$3"
  local VER="$4"
  rm -rf "$OUT"
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" \
    "$INPUTS/claude-code-$VER/binary-decompiled/src/entrypoints/index.js" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    --llm-cache "$CACHE" --prior-version "$PRIOR" \
    -vv --log-file "$LOG" > "$LOG.stdout" 2>&1
}

BEFORE=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "cache entries before: $BEFORE"

REBASE="$WORK/exp050-215-rebased"
echo "=== rebasing $FROM with the current tree (so the prior carries emitNames) ==="
run "$REBASE" "$PRIORS/claude-code-$FROM/.humanify/humanified.js" "$WORK/exp050-rebase.log" "$FROM"
node -e 'const l=require(process.argv[1]);console.log("  prior ledger emitNames:", Array.isArray(l.emitNames)?l.emitNames.length+" entries":"ABSENT")' \
  "$REBASE/.humanify/split-ledger.json" || true

PRIOR="$REBASE/.humanify/humanified.js"
echo "=== leg ON — keying on (hash, name) ==="
HUMANIFY_NO_NAME_ALIGN="" run "$WORK/exp050-on" "$PRIOR" "$RESULTS/exp050-on.log" "$TO"
MID=$(find "$CACHE" -type f 2>/dev/null | wc -l); echo "  cache written: $((MID-BEFORE))"
echo "=== leg OFF — hash-only (pre-050) ==="
HUMANIFY_NO_NAME_ALIGN=1 run "$WORK/exp050-off" "$PRIOR" "$RESULTS/exp050-off.log" "$TO"
AFTER=$(find "$CACHE" -type f 2>/dev/null | wc -l); echo "  cache written: $((AFTER-MID))  <- near-zero means the legs are comparable"

echo "################ EXACT DELTA ################"
npx tsx "$REPO/experiments/037-noise-source-decomposition/diff-composition.ts" \
  "$REBASE/src" "$WORK/exp050-off/src" "OFF (hash only)"
npx tsx "$REPO/experiments/037-noise-source-decomposition/diff-composition.ts" \
  "$REBASE/src" "$WORK/exp050-on/src" "ON (hash+name)"
echo "MEASURE COMPLETE"
