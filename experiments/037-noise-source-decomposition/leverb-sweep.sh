#!/usr/bin/env bash
# Full Lever-B sweep over the eval's version pairs. For each FROM->TO:
#   1. rebase FROM with the current tree (align ON)      -> the prior tree
#   2. humanify TO align ON  (Lever B)                    -> the shipped tree
#   3. humanify TO align OFF (HUMANIFY_NO_EMIT_ALIGN=1)   -> the A/B baseline
#   4. boot ON, self-hop ON, and measure reorder + git churn of ON vs OFF vs prior
# Everything lands under $WORK/leverb-sweep/<TO>-{rebased,on,off}. Prints one
# table row per pair + a machine-readable ROW| line the table builder consumes.
#
# Long (full pipeline runs; cold-cache pairs hit the LLM). Run detached.
#   experiments/037-noise-source-decomposition/leverb-sweep.sh 2>&1 | tee sweep.log
set -uo pipefail
REPO=/Users/andrewgross/Development/humanify
CFG=$REPO/experiments/034-eval-harness/pairs.json
INPUTS=$(jq -r .inputsBase "$CFG"); PRIORS=$(jq -r .priorsBase "$CFG")
ENDPOINT=$(jq -r .llm.endpoint "$CFG"); MODEL=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG"); EFFORT=$(jq -r .llm.reasoningEffort "$CFG"); CONC=$(jq -r .llm.concurrency "$CFG")
CACHE=/tmp/eval-work/llm-cache
W=/tmp/eval-work/leverb-sweep; mkdir -p "$W"
HEAP="--max-old-space-size=14336"
inp() { echo "$INPUTS/claude-code-$1/binary-decompiled/src/entrypoints/index.js"; }
hum() { echo "$1/.humanify/humanified.js"; }
run() { # <align_env> <input> <out> <prior>
  local env=$1; shift
  env HUMANIFY_NO_EMIT_ALIGN="$env" NODE_OPTIONS="$HEAP" npx tsx "$REPO/src/index.ts" "$1" \
    --split --endpoint "$ENDPOINT" --model "$MODEL" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$2" --llm-cache "$CACHE" \
    --prior-version "$3" -vv --log-file "$2.log" > "$2.stdout" 2>&1
}
pairs="${PAIRS:-2.1.85:2.1.86 2.1.118:2.1.119 2.1.197:2.1.198 2.1.215:2.1.216}"
echo "pair|boot|selfhop|reorderOFF|reorderON|gitOFF|gitON|mismatch"
for p in $pairs; do
  FROM=${p%%:*}; TO=${p##*:}
  REB=$W/$TO-rebased; ON=$W/$TO-on; OFF=$W/$TO-off
  ARCH="$PRIORS/claude-code-$FROM/.humanify/humanified.js"
  echo "=== $FROM->$TO: rebase prior $(date +%H:%M:%S) ===" >&2
  run 0 "$(inp "$FROM")" "$REB" "$ARCH"; [ -f "$(hum "$REB")" ] || { echo "$FROM->$TO|REBASE-FAIL"; continue; }
  echo "=== $TO: align ON $(date +%H:%M:%S) ===" >&2
  run 0 "$(inp "$TO")" "$ON" "$(hum "$REB")"; [ -f "$(hum "$ON")" ] || { echo "$FROM->$TO|ON-FAIL"; continue; }
  echo "=== $TO: align OFF $(date +%H:%M:%S) ===" >&2
  run 1 "$(inp "$TO")" "$OFF" "$(hum "$REB")"; [ -f "$(hum "$OFF")" ] || { echo "$FROM->$TO|OFF-FAIL"; continue; }
  # boot ON
  boot="n/a"; [ -f "$ON/run.cjs" ] && boot=$( (cd "$ON" && timeout 60 bun run.cjs --version 2>&1 | tail -1) || true )
  # self-hop ON
  SELF=$W/$TO-selfhop; run 0 "$(inp "$TO")" "$SELF" "$(hum "$ON")"
  sh="n/a"; [ -f "$(hum "$SELF")" ] && { cmp -s "$(hum "$ON")" "$(hum "$SELF")" && sh=0 || sh=$(diff "$(hum "$ON")" "$(hum "$SELF")" | grep -c '^[<>]'); }
  bash "$REPO/experiments/037-noise-source-decomposition/leverb-measure.sh" "$ON" "$OFF" "$REB/src" "$FROM->$TO" 2>/dev/null | grep '^ROW|' | \
    awk -F'|' -v b="$boot" -v s="$sh" '{print "TABLEROW|"$2"|"b"|"s"|"$4"|"$5"|"$6"|"$7"|"$8}'
done
echo "=== sweep done $(date +%H:%M:%S) ===" >&2
