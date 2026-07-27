#!/usr/bin/env bash
#
# 049 — the EXACT effect of admitting the export registrar, isolated from LLM draws.
#
#   experiments/049-reorder-drivers/measure-registrar.sh [workdir]
#
# WHY THIS IS NOT A RULE-10 VIOLATION. The cold A/B established that the src/
# per-hop draw band is +/-2,800 lines while this pass's effect is <=467 across
# four hops, so the harness cannot resolve it: two hops FAILED "noiseLn down"
# having shipped zero moves, and the biggest apparent win was 91% draw variance.
#
# The pass itself is a DETERMINISTIC post-render transform. Rule 10 forbids the
# cache for a verdict about LLM-dependent behaviour and explicitly permits it for
# "probing a deterministic surface" — which is exactly this. With every rename
# prompt replayed, both legs render the SAME pre-pass bundle, so the delta IS the
# pass rather than a ceiling on it.
#
# ORDER IS LOad-BEARING: leg 1 (ON) runs first and POPULATES the cache; leg 2
# (OFF) then replays it, which is what makes its pre-pass render identical. Run
# them the other way round and leg 2's prompts miss and draw live.
#
# WHAT THIS CANNOT SEE, and it must be stated with the number: the pass's output
# becomes the NEXT release's prior, so a multi-hop feedback effect exists that one
# pinned pair cannot show. The cold four-pair A/B is the evidence about that, and
# it found no attributable determinism cost.
set -uo pipefail

WORK="${1:-/work}"
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
cd "$REPO"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
CACHE="${ISOLATION_CACHE:-$WORK/llm-cache}"

FROM=2.1.215
TO=2.1.216
PAIR="$FROM->$TO"
INPUTS="${EVAL_INPUTS_BASE:-$(jq -r .inputsBase "$CFG")}"
INPUT="$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"

# ONE fixed prior for both legs, so the only variable is the flag. The cold
# control's rebased 215 is the right base: current-pipeline formatting (the
# reason REBASE_PRIOR exists) and produced with the pass OFF, so it favours
# neither leg.
PRIOR_BASE="$WORK/exp048-cold-control/$FROM-rebased"
PRIOR="$PRIOR_BASE/.humanify/humanified.js"
PRIOR_LEDGER="$PRIOR_BASE/.humanify/split-ledger.json"

[[ -f "$PRIOR" ]] || { echo "FATAL: no prior at $PRIOR" >&2; exit 1; }
[[ -f "$INPUT" ]] || { echo "FATAL: no input at $INPUT" >&2; exit 1; }

ENDPOINT="${EVAL_ENDPOINT:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")
RESULTS="$REPO/experiments/034-eval-harness/results"

run_leg() {
  # Separate statements on purpose: in a single `local a=... b=$a`, bash has
  # already declared `a` as an unset local when `$a` expands, which trips set -u.
  local LABEL="$1"
  local OUT="$WORK/$LABEL"
  mkdir -p "$RESULTS/$LABEL"
  echo "=== leg $LABEL (HUMANIFY_NO_REGISTRAR_EXEMPTION='${HUMANIFY_NO_REGISTRAR_EXEMPTION:-}') ==="
  rm -rf "$OUT"
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    --llm-cache "$CACHE" --prior-version "$PRIOR" \
    --stats-json "$RESULTS/$LABEL/$TO.stats.json" \
    -vv --log-file "$RESULTS/$LABEL/$TO.log" \
    > "$RESULTS/$LABEL/$TO.stdout" 2>&1
  [[ -f "$OUT/.humanify/humanified.js" ]] || { echo "PIPELINE FAILED: $LABEL"; return 1; }
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx \
    "$REPO/experiments/034-eval-harness/analyze.ts" \
    "$OUT/.humanify/humanified.js" "$PRIOR" \
    "$OUT/.humanify/split-ledger.json" "$PRIOR_LEDGER" \
    "$RESULTS/$LABEL/$TO.stats.json" "$PAIR" \
    "$OUT/src" "$PRIOR_BASE/src" "$OUT/vendor" "$PRIOR_BASE/vendor" \
    > "$RESULTS/$LABEL/$TO.json" || echo "ANALYZE FAILED: $LABEL"
}

BEFORE=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "cache entries before: $BEFORE"

# Leg 1 FIRST and with the pass ON: it draws whatever is missing and writes it,
# so leg 2 can replay every rename prompt. The empty value is deliberate — the
# plugin tests the variable for truthiness, and "" is falsy, so the pass runs.
HUMANIFY_NO_REGISTRAR_EXEMPTION="" run_leg exp049-reg-on
MID=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "cache entries after leg 1 (ON): $MID  (+$((MID - BEFORE)) written)"

HUMANIFY_NO_REGISTRAR_EXEMPTION=1 run_leg exp049-reg-off
AFTER=$(find "$CACHE" -type f 2>/dev/null | wc -l)
echo "cache entries after leg 2 (OFF): $AFTER  (+$((AFTER - MID)) written)"
echo
echo "The leg-2 write count is the KEY DIAGNOSTIC: near-zero means its prompts"
echo "replayed, so both legs rendered the same pre-pass bundle and the delta is"
echo "the pass. A large count means draws leaked in and the isolation FAILED."

echo
echo "################ EXACT DELTA ################"
npx tsx -e "
const p='$RESULTS/';
const on=require(p+'exp049-reg-on/$TO.json').churn, off=require(p+'exp049-reg-off/$TO.json').churn;
const row=(k,a,b)=>console.log('  '+k.padEnd(14)+String(a).padStart(9)+String(b).padStart(11)+String(b-a).padStart(9));
console.log('  metric             OFF        ON     delta');
row('layout.noise', off.layout.noise, on.layout.noise);
row('layout.naming', off.layout.naming, on.layout.naming);
row('layout.reorder', off.layout.reorder, on.layout.reorder);
row('layout.alias', off.layout.alias, on.layout.alias);
row('layout.real', off.layout.real, on.layout.real);
row('noiseLn', off.lines.namingNoiseLines, on.lines.namingNoiseLines);
row('realLn', off.lines.realLines, on.lines.realLines);
row('novel', off.statements.novel, on.statements.novel);
row('noise(st)', off.statements.unchangedChurned, on.statements.unchangedChurned);
"
echo
echo "(reorder is emit order; no moves trail applies here) suppressed count: $(grep -acE 'move [^ ]+ -> [^ ]+ \(support' "$RESULTS/exp049-reg-on/$TO.log" 2>/dev/null)"
echo "EXACT ISOLATION COMPLETE"
