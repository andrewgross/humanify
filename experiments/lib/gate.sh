#!/usr/bin/env bash
#
# exp054 — the EXACT effect of the post-split reconcile pass, four pairs,
# isolated from LLM draws.
#
#   experiments/054-post-split-reconcile/pinned-ab.sh [workdir]
#
# WHY A PINNED RUN AND NOT A COLD A/B. The ceiling, measured on trees already
# on disk before a line of this pass was written, is 1,162 / 80 / 2,028 / 1,674
# git lines. The src/ per-hop draw band is +/-2,800 (exp048, rule 11), so a cold
# A/B cannot resolve ANY of the four hops and would still print a confident
# sign for each. Rule 10 forbids the cache for a verdict about LLM-dependent
# behaviour and explicitly permits it for a deterministic surface; this pass is
# deterministic and sits AFTER every prompt — the last thing before the tree is
# written — so with the prompts replayed both legs render the same pre-pass
# tree and the delta IS the pass.
#
# ORDER IS LOAD-BEARING: leg ON runs first and POPULATES the cache; leg OFF
# then replays it. The leg-OFF write count is the key diagnostic — near zero
# means the isolation held, a large count means draws leaked in and the numbers
# mean nothing.
#
# WHAT THIS CANNOT SEE, and it must be stated with the number:
#   1. Multi-hop feedback. In production this release's tree becomes the next
#      release's prior, and its `.humanify/humanified.js` does NOT carry the
#      post-split renames (they are computed per split file and cannot be
#      applied to a bundle by name). The prior TREE and the prior BUNDLE
#      therefore disagree by exactly this pass's renames. Each leg here uses a
#      fixed pass-OFF prior, so one hop of that feedback is unmeasured.
#   2. Draw-dependent interactions. With prompts pinned, a rename that would
#      have changed which name the LLM proposes elsewhere cannot show up.
set -uo pipefail

# Flags (parsed upfront; unknown flags fatal — no ambient env reads):
#   [workdir]           positional, default /work
#   --flag <switch>     registry switch the OFF leg ablates (--disable name)
#   --trail <marker>    log marker proving the pass fired on this hop
#   --pairs "<a:b ...>" space-separated from:to pairs to gate
#   --tag <label>       results label prefix
#   --cache <dir>       pinned LLM cache (default <workdir>/exp054-cache)
#   --priors <dir>      prior-tree root (default <workdir>/exp050-cold)
#   --inputs-base <dir> override pairs.json inputsBase
#   --endpoint <url>    override pairs.json endpoint
WORK="/work"
FLAG="post-split-reconcile"
TRAIL="post-split-reconcile"
PAIRS_ARG=""
TAG="exp054"
CACHE_OVERRIDE=""
PRIORS_OVERRIDE=""
INPUTS_OVERRIDE=""
ENDPOINT_OVERRIDE=""
POSN=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --flag)        FLAG="$2"; shift ;;
    --trail)       TRAIL="$2"; shift ;;
    --pairs)       PAIRS_ARG="$2"; shift ;;
    --tag)         TAG="$2"; shift ;;
    --cache)       CACHE_OVERRIDE="$2"; shift ;;
    --priors)      PRIORS_OVERRIDE="$2"; shift ;;
    --inputs-base) INPUTS_OVERRIDE="$2"; shift ;;
    --endpoint)    ENDPOINT_OVERRIDE="$2"; shift ;;
    --*)           echo "gate.sh: unknown flag $1" >&2; exit 2 ;;
    *)             if [[ $POSN -gt 0 ]]; then echo "gate.sh: unexpected arg $1" >&2; exit 2; fi
                   WORK="$1"; POSN=1 ;;
  esac
  shift
done
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
cd "$REPO"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
CACHE="${CACHE_OVERRIDE:-$WORK/exp054-cache}"
PRIOR_ROOT="${PRIORS_OVERRIDE:-$WORK/exp050-cold}"
RESULTS="$REPO/experiments/034-eval-harness/results"

INPUTS="${INPUTS_OVERRIDE:-$(jq -r .inputsBase "$CFG")}"
ENDPOINT="${ENDPOINT_OVERRIDE:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")

# The boot gate lives in experiments/lib and is FATAL when bun is missing —
# this script used to print a warning and carry on, which is a gate that
# reports success having verified nothing.
source "$HERE/boot-gate.sh"

PAIRS="${PAIRS_ARG:-2.1.85:2.1.86 2.1.118:2.1.119 2.1.197:2.1.198 2.1.215:2.1.216}"

# The kill switch under test, and the log marker that proves the change did
# something on THIS hop (rule 11: a pass with an empty trail cannot have moved a
# KPI, however the KPI reads). Parameterised rather than forked, so a second
# experiment reusing this gate reuses the isolation argument with it — the
# reasoning above is about a deterministic surface downstream of every prompt,
# and any pass this runs must satisfy that or the pinning is not licensed.
# FLAG/TRAIL are --flag/--trail above: the registry switch the OFF leg
# ablates (passed to the pipeline as --disable) and the log marker that
# proves it fired on this hop.

run_leg() {
  local LABEL="$1" TO="$2" PRIOR="$3" OUT
  OUT="$WORK/$LABEL"
  mkdir -p "$RESULTS/$LABEL"
  rm -rf "$OUT"
  local INPUT="$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"
  [[ -f "$INPUT" ]] || { echo "FATAL: no input at $INPUT" >&2; return 1; }
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    --llm-cache "$CACHE" --prior-version "$PRIOR" \
    ${ABLATE_ARGS[@]+"${ABLATE_ARGS[@]}"} \
    --stats-json "$RESULTS/$LABEL/$TO.stats.json" \
    -vv --log-file "$RESULTS/$LABEL/$TO.log" \
    > "$RESULTS/$LABEL/$TO.stdout" 2>&1
  [[ -f "$OUT/.humanify/humanified.js" ]] || { echo "PIPELINE FAILED: $LABEL"; return 1; }
}

run_leg_on()  { ABLATE_ARGS=(); run_leg "$@"; }
run_leg_off() { ABLATE_ARGS=(--disable "$FLAG"); local r; run_leg "$@"; r=$?; ABLATE_ARGS=(); return $r; }

analyze_leg() {
  local LABEL="$1" TO="$2" PAIR="$3" PRIOR_BASE="$4" OUT
  OUT="$WORK/$LABEL"
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx \
    "$REPO/experiments/034-eval-harness/analyze.ts" \
    "$OUT/.humanify/humanified.js" "$PRIOR_BASE/.humanify/humanified.js" \
    "$OUT/.humanify/split-ledger.json" "$PRIOR_BASE/.humanify/split-ledger.json" \
    "$RESULTS/$LABEL/$TO.stats.json" "$PAIR" \
    "$OUT/src" "$PRIOR_BASE/src" "$OUT/vendor" "$PRIOR_BASE/vendor" \
    > "$RESULTS/$LABEL/$TO.json" || echo "ANALYZE FAILED: $LABEL"
}

echo "cache dir: $CACHE"
for SPEC in $PAIRS; do
  FROM="${SPEC%%:*}"; TO="${SPEC##*:}"
  PAIR="$FROM->$TO"
  PRIOR_BASE="$PRIOR_ROOT/$FROM-rebased"
  PRIOR="$PRIOR_BASE/.humanify/humanified.js"
  [[ -f "$PRIOR" ]] || { echo "FATAL: no prior at $PRIOR" >&2; exit 1; }
  echo
  echo "################ $PAIR ################"
  BEFORE=$(find "$CACHE" -type f 2>/dev/null | wc -l)

  # Leg ON first: it draws whatever is missing and writes it, so leg OFF can
  # replay every prompt. Empty string is deliberate — the pass tests for "1".
  echo "--- leg ON  ($PAIR)   flag: $FLAG"
  run_leg_on "$TAG-on-$TO" "$TO" "$PRIOR" || continue
  MID=$(find "$CACHE" -type f 2>/dev/null | wc -l)
  echo "cache written by leg ON:  $((MID - BEFORE))"

  echo "--- leg OFF ($PAIR)"
  run_leg_off "$TAG-off-$TO" "$TO" "$PRIOR" || continue
  AFTER=$(find "$CACHE" -type f 2>/dev/null | wc -l)
  echo "cache written by leg OFF: $((AFTER - MID))   <-- KEY DIAGNOSTIC (must be ~0)"

  # The write count is a PROXY for isolation; this is the thing itself. exp058's
  # first gate run had leg OFF write 16 and 11 entries on two hops, whose bundles
  # then differed by 204 and 44 lines — and the harness still printed a confident
  # -144 delta for a hop whose mechanism-derived prediction was 0. A pass that
  # never touches the bundle can only be isolated if both legs entered it from
  # the same bundle, so say whether they did. Re-running both legs against the
  # now-warm cache brought both hops to 0 written, identical bundles, and a
  # delta of exactly 0.
  ON_BUNDLE="$WORK/$TAG-on-$TO/.humanify/humanified.js"
  OFF_BUNDLE="$WORK/$TAG-off-$TO/.humanify/humanified.js"
  if cmp -s "$ON_BUNDLE" "$OFF_BUNDLE"; then
    echo "bundles ON vs OFF: IDENTICAL   <-- the delta below IS the change"
  else
    echo "bundles ON vs OFF: DIFFER by $(diff "$ON_BUNDLE" "$OFF_BUNDLE" | grep -cE '^[<>]') lines" \
         "  <-- NOT ISOLATED: this hop's delta is draw-contaminated, re-run both legs warm"
  fi

  analyze_leg "$TAG-on-$TO"  "$TO" "$PAIR" "$PRIOR_BASE"
  analyze_leg "$TAG-off-$TO" "$TO" "$PAIR" "$PRIOR_BASE"
  boot_gate "$WORK/$TAG-on-$TO"  "$TO"
  boot_gate "$WORK/$TAG-off-$TO" "$TO"

  echo "$TRAIL lines, ON leg:  $(grep -ac "$TRAIL" "$RESULTS/$TAG-on-$TO/$TO.log" 2>/dev/null || echo 0)"
  echo "$TRAIL lines, OFF leg: $(grep -ac "$TRAIL" "$RESULTS/$TAG-off-$TO/$TO.log" 2>/dev/null || echo 0)"
done

echo
echo "################ PINNED DELTAS ################"
npx tsx "$REPO/experiments/054-post-split-reconcile/pinned-report.ts" "$RESULTS" "$PAIRS" "$WORK" "$PRIOR_ROOT" "$TAG"
echo "PINNED A/B COMPLETE"
