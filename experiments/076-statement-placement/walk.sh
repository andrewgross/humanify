#!/usr/bin/env bash
#
# 076 — A/B the settled-anchor pass over a REAL VERSION WALK.
#
#   experiments/076-statement-placement/walk.sh <workdir> [--disable <switch>]
#
# WHY THIS AND NOT THE EVAL (Andrew, 2026-08-16: "we need to do a real
# version walk... when testing big changes like this we always need to do a
# fresh run from scratch").
#
# The eval builds each pair's base by re-humanifying version N against the
# ARCHIVE prior — a tree from the pre-fossil pipeline whose ledger carries no
# `fossilModules`. So the base has ZERO module matches and places every module
# by inference, while the scored tree inherits and anchors. The settled-anchor
# pass is then measured on an asymmetry that no real release ever sees, and it
# came out +28,745 tree lines against a band of 14.
#
# A walk has no such asymmetry. One cold start, then every hop inherits the
# tree the previous hop produced — which is what actually happens release to
# release. The hops before the measured one exist to WARM the chain: by the
# time the last pair is scored, both its trees descend from fossil ledgers.
#
#   2.1.213  cold, no prior          <- the only inference-only tree
#   2.1.214  prior = 213             <- chain warming
#   2.1.215  prior = 214             <- both trees below now inherit
#   2.1.216  prior = 215             <- MEASURED: 214->215 and 215->216
#
# Run it twice, once with `--disable fossil-settled-anchor`, and diff the two
# scored pairs. Everything else is held: same versions, same order, same
# endpoint, no LLM cache (rule 10 — a gate run reads live prompts or it reads
# nothing).
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
CFG="$REPO/experiments/034-eval-harness/pairs.json"

WORK="${1:?usage: walk.sh <workdir> [--disable <switch>]}"
shift || true
DISABLE=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --disable) DISABLE="$2"; shift ;;
    *) echo "walk.sh: unknown arg $1" >&2; exit 2 ;;
  esac
  shift
done

VERSIONS=(2.1.213 2.1.214 2.1.215 2.1.216)
INPUTS="$(jq -r .inputsBase "$CFG")"
ENDPOINT="$(jq -r .llm.endpoint "$CFG")"
MODELNAME="$(jq -r .llm.model "$CFG")"
APIKEY="$(jq -r .llm.apiKey "$CFG")"
EFFORT="$(jq -r .llm.reasoningEffort "$CFG")"
CONC="$(jq -r .llm.concurrency "$CFG")"
HEAP=65536

# bun on PATH or the boot gate silently skips (project folklore, CLAUDE.md).
source "$REPO/experiments/lib/boot-gate.sh"

mkdir -p "$WORK"
DISABLE_ARGS=()
[[ -n "$DISABLE" ]] && DISABLE_ARGS=(--disable "$DISABLE")
echo "walk: ${VERSIONS[*]}"
echo "walk: workdir $WORK, disable='${DISABLE:-none}'"

PRIOR=""
for V in "${VERSIONS[@]}"; do
  IN="$INPUTS/claude-code-$V/binary-decompiled/src/entrypoints/index.js"
  OUT="$WORK/$V"
  if [[ ! -f "$IN" ]]; then echo "walk: MISSING INPUT $IN" >&2; exit 1; fi
  PRIOR_ARGS=()
  [[ -n "$PRIOR" ]] && PRIOR_ARGS=(--prior-version "$PRIOR")
  echo "=== $(date -u +%H:%M:%S) hop $V (prior='${PRIOR:-COLD}') ==="
  rm -rf "$OUT"
  NODE_OPTIONS="--max-old-space-size=$HEAP" npx tsx "$REPO/src/index.ts" "$IN" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    --stats-json "$WORK/$V.stats.json" \
    ${PRIOR_ARGS[@]+"${PRIOR_ARGS[@]}"} ${DISABLE_ARGS[@]+"${DISABLE_ARGS[@]}"} \
    -vv --log-file "$WORK/$V.log" > "$WORK/$V.stdout" 2>&1
  RC=$?
  echo "  exit $RC"
  if [[ ! -f "$OUT/.humanify/humanified.js" ]]; then
    echo "walk: hop $V produced no bundle — aborting" >&2
    exit 1
  fi
  PRIOR="$OUT/.humanify/humanified.js"
done

# The measured pairs: both trees descend from a fossil ledger. 213->214 is
# NOT scored — 213 is the cold tree and carries the same asymmetry the eval
# has, which is the thing being controlled for.
# Scored with the SAME analyzer the eval uses, so the numbers are directly
# comparable to exp074-r1 / exp076-head-* — only the base differs, which is
# the whole point of the experiment.
echo "=== $(date -u +%H:%M:%S) scoring warm pairs ==="
for PAIR in "2.1.214 2.1.215" "2.1.215 2.1.216"; do
  set -- $PAIR
  FROM="$1"; TO="$2"
  NODE_OPTIONS="--max-old-space-size=$HEAP" npx tsx \
    "$REPO/experiments/034-eval-harness/analyze.ts" \
    "$WORK/$TO/.humanify/humanified.js" "$WORK/$FROM/.humanify/humanified.js" \
    "$WORK/$TO/.humanify/split-ledger.json" \
    "$WORK/$FROM/.humanify/split-ledger.json" \
    "$WORK/$TO.stats.json" "$FROM->$TO" \
    "$WORK/$TO/src" "$WORK/$FROM/src" \
    > "$WORK/card-$FROM-$TO.json" || echo "  ANALYZE FAILED $FROM->$TO"
  echo "--- $FROM -> $TO ---"
  jq -c '{churn:.churn.tree, layout:.churn.layout}' "$WORK/card-$FROM-$TO.json" \
    2>/dev/null || echo "  (no card)"
done
echo "SENTINEL: WALK-DONE"
