#!/usr/bin/env bash
# exp038 validation: re-run each hop with dependency-aware emit order (align ON)
# against the EXISTING rebased prior and align-OFF baseline from the exp037
# sweep, so the only thing that changed is this pass. Per hop:
#   1. humanify TO, align ON, prior = <TO>-rebased, shared LLM cache
#   2. gates: pure-reorder vs OFF, boot, naming byte-identical vs OFF
#   3. churn vs the prior tree + diff composition
#   4. self-hop: re-run with its own output as prior -> must be byte-identical
# Judge every hop on its own; 118->119 is the regression canary.
#
#   PAIRS="2.1.118:2.1.119" experiments/038-dependency-aware-reorder/run-hops.sh
set -uo pipefail
REPO=/Users/andrewgross/Development/humanify
E37=$REPO/experiments/037-noise-source-decomposition
E38=$REPO/experiments/038-dependency-aware-reorder
CFG=$REPO/experiments/034-eval-harness/pairs.json
INPUTS=$(jq -r .inputsBase "$CFG")
ENDPOINT=$(jq -r .llm.endpoint "$CFG"); MODEL=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG"); EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")
CACHE=/tmp/eval-work/llm-cache
W=/tmp/eval-work/leverb-sweep          # rebased priors + OFF baselines live here
V=/tmp/eval-work/exp038; mkdir -p "$V"
HEAP="--max-old-space-size=14336"
TAG=${TAG:-v3}

inp() { echo "$INPUTS/claude-code-$1/binary-decompiled/src/entrypoints/index.js"; }
hum() { echo "$1/.humanify/humanified.js"; }
run() { # <input> <out> <prior>
  NODE_OPTIONS="$HEAP" npx tsx "$REPO/src/index.ts" "$1" \
    --split --endpoint "$ENDPOINT" --model "$MODEL" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$2" --llm-cache "$CACHE" \
    --prior-version "$3" -vv --log-file "$2.log" > "$2.stdout" 2>&1
}
gc() { git diff --no-index --numstat "$1" "$2" 2>/dev/null | awk '$1!="-"&&$2!="-"{a+=$1;d+=$2} END{print a+d}'; }

pairs="${PAIRS:-2.1.118:2.1.119 2.1.215:2.1.216 2.1.85:2.1.86 2.1.197:2.1.198}"
for p in $pairs; do
  FROM=${p%%:*}; TO=${p##*:}
  # NOTE: $W/<v>-on holds the superseded Lever B **v1** trees; the current
  # shipped behaviour (v2, with the unambiguous-hash guard) is /tmp/eval-work/
  # leverb/<n>-on-v2. Compare against v2 or the bar is too low.
  REB=$W/$TO-rebased; OFF=$W/$TO-off; V2=/tmp/eval-work/leverb/${TO#2.1.}-on-v2; ON=$V/$TO-$TAG
  echo "########## $FROM -> $TO ($TAG)  $(date +%H:%M:%S) ##########"
  for d in "$REB" "$OFF" "$V2"; do
    [ -d "$d/src" ] || { echo "MISSING baseline $d — re-run the exp037 sweep"; continue 2; }
  done
  run "$(inp "$TO")" "$ON" "$(hum "$REB")"
  [ -f "$(hum "$ON")" ] || { echo "$FROM->$TO|ON-FAIL (see $ON.stdout)"; continue; }

  # --- naming must be untouched: same prior + cache => identical bundle ---
  if cmp -s "$(hum "$ON")" "$(hum "$OFF")"; then naming=IDENTICAL; else naming=DRIFTED; fi
  echo "  naming vs OFF: $naming  (DRIFTED invalidates the comparison)"

  # --- pure reorder + boot + churn (shared exp037 gate script) ---
  bash "$E37/leverb-measure.sh" "$ON" "$OFF" "$REB/src" "$FROM->$TO($TAG)" 2>/dev/null

  # --- vs the CURRENT shipped behaviour (Lever B v2), the bar to beat ---
  echo "  git churn vs prior: v2=$(gc "$REB/src" "$V2/src")  v3=$(gc "$REB/src" "$ON/src")"
  NODE_OPTIONS="--max-old-space-size=12288" npx tsx "$E37/diff-composition.ts" \
    "$REB/src" "$ON/src" "$FROM->$TO($TAG)" 2>/dev/null | grep -E "accounted|REAL|noise|naming|alias|reorder|^ROW"

  # --- self-hop: its own output as prior must reproduce it byte-identically ---
  SELF=$V/$TO-$TAG-selfhop
  run "$(inp "$TO")" "$SELF" "$(hum "$ON")"
  if [ -f "$(hum "$SELF")" ]; then
    if cmp -s "$(hum "$ON")" "$(hum "$SELF")"; then sh=0; else sh=$(diff "$(hum "$ON")" "$(hum "$SELF")" | grep -c '^[<>]'); fi
    st=$(diff -rq "$ON/src" "$SELF/src" 2>/dev/null | wc -l | tr -d ' ')
    echo "  self-hop: bundle diff lines=$sh  differing src files=$st"
  else
    echo "  self-hop: FAILED to produce output"
  fi
  echo "########## $FROM -> $TO done $(date +%H:%M:%S) ##########"
done
