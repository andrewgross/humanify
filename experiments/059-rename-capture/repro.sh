#!/usr/bin/env bash
#
# exp059 — reproduce the rename capture on 2.1.197->2.1.198.
#
#   experiments/059-rename-capture/repro.sh [attempts] [workdir]
#   experiments/059-rename-capture/repro.sh 14
#
# The bug fires on ~20% of COLD runs (8 of 40 committed 2.1.198 runs). It has
# never reproduced warm. So this runs the pair cold, repeatedly, and STOPS at
# the first hit — one hit is all you need, and each run costs ~16 minutes.
#
#   10 attempts ~= 89% chance of at least one hit   (1 - 0.8^10)
#   14 attempts ~= 96%                              (1 - 0.8^14)
#
# WHY COLD, and why you must not "speed it up" with the cache: the capture is
# draw-dependent — the model has to propose a colliding name. A warm cache
# replays fixed answers, which is exactly the condition under which this has
# never fired. It is also what measurement-pitfalls rule 10 requires for any
# verdict.
#
# ON A HIT this prints the preserved evidence path. Both files matter:
#   <out>/.humanify/failed/runtime.js            the renamed output
#   <out>/.humanify/failed/runtime.js.original   the PRE-rename source
# A capture is invisible from one side — `x !== x` in the output alone is the
# standard NaN idiom and appears throughout real bundles.
set -uo pipefail

ATTEMPTS="${1:-14}"
WORK="${2:-/work/exp059}"

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
cd "$REPO"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
command -v jq >/dev/null || { echo "jq required"; exit 1; }

INPUTS="${EVAL_INPUTS_BASE:-$(jq -r .inputsBase "$CFG")}"
PRIORS="${EVAL_PRIORS_BASE:-$(jq -r .priorsBase "$CFG")}"
ENDPOINT="${EVAL_ENDPOINT:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")
HEAP="${EVAL_HEAP:-65536}"

INPUT="$INPUTS/claude-code-2.1.198/binary-decompiled/src/entrypoints/index.js"
PRIOR="$PRIORS/claude-code-2.1.197/.humanify/humanified.js"

[[ -f "$INPUT" ]] || { echo "FATAL: no input at $INPUT" >&2; exit 1; }
[[ -f "$PRIOR" ]] || { echo "FATAL: no prior at $PRIOR" >&2; exit 1; }

# An ambient HUMANIFY_LLM_CACHE would silently re-pin the run to a cache and
# guarantee it never fires — the same trap run.sh guards against.
unset HUMANIFY_LLM_CACHE

mkdir -p "$WORK"
echo "=== exp059 capture repro: up to $ATTEMPTS COLD runs of 2.1.197->2.1.198 ==="
echo "    commit $(git rev-parse --short HEAD)$([[ -n "$(git status --porcelain --untracked-files=no)" ]] && echo ' (DIRTY — the run corresponds to no commit)')"
echo "    ~20% per run; ~16 min each. Stops at the first hit."
echo

for i in $(seq 1 "$ATTEMPTS"); do
  OUT="$WORK/hit-$i"
  rm -rf "$OUT"
  printf "[%2d/%s] " "$i" "$ATTEMPTS"
  NODE_OPTIONS="--max-old-space-size=$HEAP" npx tsx "$REPO/src/index.ts" "$INPUT" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    --prior-version "$PRIOR" \
    --diagnostics "$WORK/hit-$i.diag.json" \
    > "$WORK/hit-$i.stdout" 2>&1
  RC=$?

  if [[ $RC -ne 0 ]] && grep -q "violated rename invariants" "$WORK/hit-$i.stdout"; then
    echo "*** HIT (exit $RC) ***"
    echo
    grep -A 6 "^ERROR:" "$WORK/hit-$i.stdout" | head -10
    echo
    echo "PRESERVED EVIDENCE — read BOTH, a capture is invisible from one side:"
    ls -la "$OUT/.humanify/failed/" 2>/dev/null | sed 's/^/    /'
    echo
    echo "Per-identifier rename trail: $WORK/hit-$i.diag.json"
    echo
    echo "Next: find a self-comparison in the output whose ORIGINAL had two"
    echo "DIFFERENT identifiers. Self-comparisons alone are the NaN idiom and"
    echo "prove nothing:"
    echo "    grep -nE '([A-Za-z_\$][A-Za-z0-9_\$]*) !== \\1\\b' $OUT/.humanify/failed/runtime.js"
    exit 0
  fi

  if [[ $RC -ne 0 ]]; then
    echo "exit $RC but NOT an invariant violation — investigate separately:"
    tail -5 "$WORK/hit-$i.stdout" | sed 's/^/      /'
    exit 1
  fi
  echo "clean"
  rm -rf "$OUT"   # keep the disk sane; only a hit is worth keeping
done

echo
echo "No hit in $ATTEMPTS runs."
echo "At ~20% per run that is a $(awk -v n="$ATTEMPTS" 'BEGIN{printf "%.1f", 100*(0.8^n)}')% outcome if the bug is still fully present."
echo "That is EVIDENCE OF ABSENCE ONLY IF you state the run count with the claim."
exit 2
