#!/usr/bin/env bash
#
# 049 — the cold A/B for admitting the export registrar as a target-writing call.
#
#   experiments/049-reorder-drivers/cold-ab.sh [workdir]
#
# Control leg = pass OFF (HUMANIFY_NO_REGISTRAR_EXEMPTION=1), candidate leg = pass ON.
# Both legs COLD: run.sh defaults to no LLM cache since exp047, and this script
# deliberately never sets EVAL_LLM_CACHE. A cached run cannot measure this
# experiment even in principle -- the change unpins 580 of the bundle's 588
# barriers, and second-order effects on naming/placement are exactly what a
# draw-pinned probe cannot see.
#
# Sequential on purpose: two concurrent evals contend for the single vLLM
# endpoint and for CPU (-c 32 each), which perturbs the very variance being
# measured.
#
# Replaces experiments/037-noise-source-decomposition/ab-pair.sh, which is stale
# (hardcodes a humanify-lever1v2 checkout, /tmp/eval-work, and a cache dir).
set -uo pipefail

WORK="${1:-/work}"
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
cd "$REPO"

# bun is NOT on PATH here; without it run.sh SILENTLY prints "BOOT GATE SKIPPED"
# and gate criterion 4 is lost without any error.
export PATH="$HOME/.bun/bin:$PATH"
if ! command -v bun >/dev/null 2>&1; then
  echo "FATAL: bun not found (expected ~/.bun/bin/bun). The boot gate would be" >&2
  echo "       silently skipped, so this run could not satisfy the gate." >&2
  exit 1
fi

# Never let an ambient cache pin the run: run.sh unsets HUMANIFY_LLM_CACHE on the
# cold path, but EVAL_LLM_CACHE would opt back IN, so refuse it explicitly here.
if [[ -n "${EVAL_LLM_CACHE:-}" ]]; then
  echo "FATAL: EVAL_LLM_CACHE is set. A gate run must be cold." >&2
  exit 1
fi

if pgrep -f 'run\.sh exp049' >/dev/null 2>&1; then
  echo "FATAL: an exp048 eval is already in flight." >&2
  exit 1
fi

CACHE_BEFORE=$(find "$WORK/llm-cache" -type f 2>/dev/null | wc -l)
STAMP=$(date '+%Y-%m-%d %H:%M:%S')
echo "cold-ab starting $STAMP  workdir=$WORK  cache entries before: $CACHE_BEFORE"

echo "################ CONTROL LEG — registrar pinned (old), COLD ################"
env HUMANIFY_NO_REGISTRAR_EXEMPTION=1 REBASE_PRIOR=1 \
  experiments/034-eval-harness/run.sh exp049-cold-control "$WORK"
echo "CONTROL LEG EXIT=$?"

echo "################ CANDIDATE LEG — registrar admitted (new), COLD ################"
env REBASE_PRIOR=1 \
  experiments/034-eval-harness/run.sh exp049-cold "$WORK"
echo "CANDIDATE LEG EXIT=$?"

# Coldness proof, recorded with the run rather than reconstructed later.
CACHE_AFTER=$(find "$WORK/llm-cache" -type f 2>/dev/null | wc -l)
NEW=$(find "$WORK/llm-cache" -type f -newermt "$STAMP" 2>/dev/null | wc -l)
echo "################ COLDNESS CHECK ################"
echo "cache entries: $CACHE_BEFORE -> $CACHE_AFTER   written during this run: $NEW"
echo "  (0 written is EXPECTED and correct: the cold path passes no --llm-cache,"
echo "   so nothing is read from or written to it. The proof that prompts reached"
echo "   the model is the vLLM counter below, NOT this number.)"
curl -s -m 5 http://host.docker.internal:8000/metrics 2>/dev/null \
  | awk '/^vllm:request_success_total/ {s+=$2} END {printf "vllm:request_success_total = %d\n", s+0}'
echo "  Compare against the value from before the run; it must have climbed by"
echo "  thousands. Also check avg call time in the logs: ~20ms means disk cache,"
echo "  hundreds of ms means live inference."

echo "################ VERDICT ################"
npx tsx experiments/034-eval-harness/leaderboard.ts exp049-cold-control exp049-cold
experiments/041-content-anchor/gate-verdict.sh exp049-cold-control exp049-cold
echo "COLD A/B COMPLETE"
