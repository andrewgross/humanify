#!/usr/bin/env bash
#
# 079 — SIZING RUN for the enclosing-statement rung's abstain reasons.
#
#   experiments/079-ambiguity/abstain-hop.sh <workdir>
#
# ONE hop, not a walk: 2.1.216 against the 2.1.215 tree the merged-main walk
# already produced. The question is which branch of
# `tryEnclosingStatementResolve` declines, and how often — a property of the
# matcher alone.
#
# THE CACHE IS LEGITIMATE HERE and this is the whole reason the run is cheap.
# Rule 10 forbids the cache for a verdict about LLM-DEPENDENT behaviour.
# Matching runs to completion before the first prompt is built, so every
# counter below is fixed before the model is consulted at all. Same licence
# exp048 used to pin draws. A verdict on a BEHAVIOUR CHANGE would need a cold
# run; this one changes no behaviour, it only records reasons.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
WORK="${1:?usage: abstain-hop.sh <workdir>}"
PRIOR_TREE="${PRIOR_TREE:-/work/walk-main/2.1.215}"
CACHE="${CACHE:-/work/llm-cache}"

INPUTS="$(jq -r .inputsBase "$CFG")"
IN="$INPUTS/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js"
[[ -f "$IN" ]] || { echo "MISSING INPUT $IN" >&2; exit 1; }
[[ -f "$PRIOR_TREE/.humanify/humanified.js" ]] || {
  echo "MISSING PRIOR $PRIOR_TREE/.humanify/humanified.js" >&2; exit 1; }

source "$REPO/experiments/lib/boot-gate.sh"
mkdir -p "$WORK"
rm -rf "$WORK/2.1.216"

NODE_OPTIONS="--max-old-space-size=65536" npx tsx "$REPO/src/index.ts" "$IN" \
  --split \
  --endpoint "$(jq -r .llm.endpoint "$CFG")" \
  --model "$(jq -r .llm.model "$CFG")" \
  --api-key "$(jq -r .llm.apiKey "$CFG")" \
  --reasoning-effort "$(jq -r .llm.reasoningEffort "$CFG")" \
  -c "$(jq -r .llm.concurrency "$CFG")" \
  --llm-cache "$CACHE" \
  --prior-version "$PRIOR_TREE/.humanify/humanified.js" \
  -o "$WORK/2.1.216" \
  --stats-json "$WORK/2.1.216.stats.json" \
  -vv --log-file "$WORK/2.1.216.log" > "$WORK/2.1.216.stdout" 2>&1
echo "exit $?"

echo "=== function cascade ==="
grep -h "function cascade:" "$WORK/2.1.216.log" || echo "  (none)"
echo "=== enclosingStmt abstained ==="
grep -h "enclosingStmt abstained:" "$WORK/2.1.216.log" || echo "  (none)"
echo "SENTINEL: ABSTAIN-HOP-DONE"
