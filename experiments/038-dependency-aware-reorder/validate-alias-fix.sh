#!/usr/bin/env bash
# exp038 Task D validation: require-alias drift (exp037 Finding 4).
#
# The alias rule changed, so the prior tree must be REGENERATED with the same
# rule or the hop measures a one-time migration instead of the steady state
# (the same reason 034's REBASE_PRIOR exists). Per pair: rebase FROM, run TO
# against it, boot, and report the alias row of the diff composition.
#
# Compare `require-alias churn` against the pre-D exp038 run: 146 lines on
# 215->216, 250 on 197->198.
set -uo pipefail
REPO=/Users/andrewgross/Development/humanify
E37=$REPO/experiments/037-noise-source-decomposition
CFG=$REPO/experiments/034-eval-harness/pairs.json
INPUTS=$(jq -r .inputsBase "$CFG"); PRIORS=$(jq -r .priorsBase "$CFG")
ENDPOINT=$(jq -r .llm.endpoint "$CFG"); MODEL=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG"); EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")
CACHE=/tmp/eval-work/llm-cache
V=/tmp/eval-work/exp038; mkdir -p "$V"
HEAP="--max-old-space-size=14336"

inp() { echo "$INPUTS/claude-code-$1/binary-decompiled/src/entrypoints/index.js"; }
hum() { echo "$1/.humanify/humanified.js"; }
run() { # <input> <out> <prior>
  NODE_OPTIONS="$HEAP" npx tsx "$REPO/src/index.ts" "$1" \
    --split --endpoint "$ENDPOINT" --model "$MODEL" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$2" --llm-cache "$CACHE" \
    --prior-version "$3" -vv --log-file "$2.log" > "$2.stdout" 2>&1
}

for p in ${PAIRS:-2.1.215:2.1.216 2.1.197:2.1.198}; do
  FROM=${p%%:*}; TO=${p##*:}
  REB=$V/$FROM-d-rebased; ON=$V/$TO-d
  echo "########## $FROM -> $TO (alias fix)  $(date +%H:%M:%S) ##########"
  run "$(inp "$FROM")" "$REB" "$PRIORS/claude-code-$FROM/.humanify/humanified.js"
  [ -f "$(hum "$REB")" ] || { echo "REBASE-FAIL — see $REB.stdout"; continue; }
  run "$(inp "$TO")" "$ON" "$(hum "$REB")"
  [ -f "$(hum "$ON")" ] || { echo "ON-FAIL — see $ON.stdout"; continue; }
  boot="n/a"; [ -f "$ON/run.cjs" ] && boot=$( (cd "$ON" && timeout 60 bun run.cjs --version 2>&1 | tail -1) || true )
  echo "  boot: $boot"
  echo "  aliases recorded in ledger: $(node -e "
    const l=require('$ON/.humanify/split-ledger.json');
    console.log(l.aliases ? Object.keys(l.aliases).length : 'MISSING');
  " 2>/dev/null)"
  NODE_OPTIONS="--max-old-space-size=12288" npx tsx "$E37/diff-composition.ts" \
    "$REB/src" "$ON/src" "$FROM->$TO(alias-fix)" 2>/dev/null |
    grep -E "accounted|REAL|noise|naming|alias|reorder|^ROW"
  echo "########## $FROM -> $TO done $(date +%H:%M:%S) ##########"
done
