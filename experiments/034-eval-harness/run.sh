#!/usr/bin/env bash
#
# Eval harness driver — score the CURRENT pipeline on the configured version
# pairs and store the result under a MODEL label so runs stack up side by side.
#
#   experiments/034-eval-harness/run.sh <model-label> [workdir]
#
# <model-label> names this run (a branch, a commit, an idea — e.g.
# "main-4117212" or "fix-close-match"). Results land in results/<model-label>/;
# `leaderboard.ts` then compares every model. Re-running a label overwrites it.
#
# One pipeline run per pair (~10-15 min each); a failed pair is logged and
# skipped, never aborts the sweep. Deterministic metrics are stable run-to-run;
# the naming-noise magnitude carries the LLM floor (see README).
set -uo pipefail

MODEL="${1:?usage: run.sh <model-label> [workdir]}"
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
WORK="${2:-/tmp/eval-work}"
CFG="$HERE/pairs.json"
RESULTS="$HERE/results/$MODEL"
mkdir -p "$RESULTS" "$WORK"

command -v jq >/dev/null || { echo "jq required"; exit 1; }

INPUTS=$(jq -r .inputsBase "$CFG")
PRIORS=$(jq -r .priorsBase "$CFG")
ENDPOINT=$(jq -r .llm.endpoint "$CFG")
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")

# Record what produced this model, for provenance.
git -C "$REPO" rev-parse --short HEAD > "$RESULTS/commit.txt" 2>/dev/null || true

npairs=$(jq '.pairs | length' "$CFG")
for i in $(seq 0 $((npairs - 1))); do
  FROM=$(jq -r ".pairs[$i].from" "$CFG")
  TO=$(jq -r ".pairs[$i].to" "$CFG")
  PAIR="$FROM->$TO"
  INPUT="$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"
  PRIOR="$PRIORS/claude-code-$FROM/.humanify/humanified.js"
  PRIOR_LEDGER="$PRIORS/claude-code-$FROM/.humanify/split-ledger.json"
  OUT="$WORK/$MODEL/$TO"
  STATS="$RESULTS/$TO.stats.json"
  LOG="$RESULTS/$TO.log"

  if [[ ! -f "$INPUT" ]]; then echo "SKIP $PAIR (no input $INPUT)"; continue; fi
  if [[ ! -f "$PRIOR" ]]; then echo "SKIP $PAIR (no prior $PRIOR)"; continue; fi

  # REBASE_PRIOR=1: a formatting change made the archive v-1 an invalid base
  # (formatting diffs would swamp the naming signal). Re-humanify the base with
  # the CURRENT pipeline (inheriting its own archive names) so the pair's diff
  # reflects naming/real change only. Costs one extra run per pair.
  if [[ "${REBASE_PRIOR:-0}" == "1" ]]; then
    INPUT_FROM="$INPUTS/claude-code-$FROM/binary-decompiled/src/entrypoints/index.js"
    if [[ -f "$INPUT_FROM" ]]; then
      REBASE="$WORK/$MODEL/${FROM}-rebased"
      echo "=== $PAIR: rebasing prior (re-humanify $FROM, current pipeline) ==="
      rm -rf "$REBASE"
      NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT_FROM" \
        --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
        --reasoning-effort "$EFFORT" -c "$CONC" -o "$REBASE" \
        --prior-version "$PRIOR" -vv --log-file "$RESULTS/${FROM}-rebase.log" \
        > "$RESULTS/${FROM}-rebase.stdout" 2>&1
      if [[ -f "$REBASE/.humanify/humanified.js" ]]; then
        PRIOR="$REBASE/.humanify/humanified.js"
        PRIOR_LEDGER="$REBASE/.humanify/split-ledger.json"
        echo "  prior rebased -> $PRIOR"
      else
        echo "  rebase FAILED; falling back to archive prior"
      fi
    fi
  fi

  echo "=== [$((i + 1))/$npairs] $PAIR: pipeline ==="
  rm -rf "$OUT"
  # Shared LLM response cache: prompts repeated across models/runs return
  # identical answers (serving-drift countermeasure, see README) and reruns
  # get dramatically cheaper. Override/disable with EVAL_LLM_CACHE.
  LLM_CACHE="${EVAL_LLM_CACHE:-$WORK/llm-cache}"
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    --llm-cache "$LLM_CACHE" \
    --prior-version "$PRIOR" --stats-json "$STATS" -vv --log-file "$LOG" \
    > "$RESULTS/$TO.stdout" 2>&1
  if [[ ! -f "$OUT/.humanify/humanified.js" ]]; then
    echo "PIPELINE FAILED for $PAIR (see $RESULTS/$TO.stdout)"; continue
  fi

  echo "=== $PAIR: churn analysis ==="
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$HERE/analyze.ts" \
    "$OUT/.humanify/humanified.js" "$PRIOR" \
    "$OUT/.humanify/split-ledger.json" "$PRIOR_LEDGER" \
    "$STATS" "$PAIR" > "$RESULTS/$TO.json" \
    || echo "ANALYZE FAILED for $PAIR"
done

echo "=== summarizing model '$MODEL' ==="
npx tsx "$HERE/summarize.ts" "$MODEL"
