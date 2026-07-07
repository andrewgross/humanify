#!/usr/bin/env bash
# Phase 2 of exp013: full end-to-end validation of the cross-version
# pipeline after the transfer-validation + binding-cascade fixes.
#
#   Run A': humanify v119 fresh (LLM-heavy)
#   Run B': humanify v120 with --prior-version <Run A' runtime.js>
#   Then:  diff stats vs the 167,944-line / 30,745-hunk baseline.
#
# Requires the local LLM endpoint to be up. Override via env:
#   HUMANIFY_ENDPOINT (default http://192.168.1.234:8000/v1)
#   HUMANIFY_MODEL    (default openai/gpt-oss-20b)
#   HUMANIFY_API_KEY  (default "local")
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
INPUTS="/Users/andrewgross/Development/claude-code-versions/inputs"
V119="$INPUTS/claude-code-2.1.119/binary-decompiled/src/entrypoints/index.js"
V120="$INPUTS/claude-code-2.1.120/binary-decompiled/src/entrypoints/index.js"
OUT="${PHASE2_OUT:-/tmp/exp013-phase2}"

ENDPOINT="${HUMANIFY_ENDPOINT:-http://192.168.1.234:8000/v1}"
MODEL="${HUMANIFY_MODEL:-openai/gpt-oss-20b}"
# A/B on the preact smoke (2026-07-06): low effort cut the smoke from
# 2m9s to 9.9s (output tokens 75.6K → 8.8K) with equal name quality and
# BETTER coverage (192/194 vs 182/194). gpt-oss reasoning channel was
# >90% of completion tokens.
REASONING_EFFORT="${HUMANIFY_REASONING_EFFORT:-low}"
export HUMANIFY_API_KEY="${HUMANIFY_API_KEY:-local}"

# LLM throughput knobs for the 4-replica fleet (~28K tok/s aggregate). Tune
# these against the run — raise if replicas sit idle, back off if client p99
# nears the 30s timeout or the failed/unrenamed count climbs. All overridable.
export HUMANIFY_CONCURRENCY="${HUMANIFY_CONCURRENCY:-120}"        # function lane
export HUMANIFY_MODULE_CONCURRENCY="${HUMANIFY_MODULE_CONCURRENCY:-40}"  # module lane
export HUMANIFY_MAX_TOKENS="${HUMANIFY_MAX_TOKENS:-2000}"        # ample w/ low reasoning

mkdir -p "$OUT"

run_humanify() {
  local input="$1" outdir="$2" diag="$3" log="$4"; shift 4
  node --max-old-space-size=16384 --import tsx/esm "$ROOT/src/index.ts" \
    "$input" -o "$outdir" \
    --diagnostics "$diag" \
    --endpoint "$ENDPOINT" --model "$MODEL" \
    --reasoning-effort "$REASONING_EFFORT" \
    --bundler bun --minifier bun \
    --log-file "$log" \
    "$@"
}

if [ ! -f "$OUT/cc-119/runtime.js" ]; then
  echo "=== Run A': humanify v119 (fresh) ==="
  time run_humanify "$V119" "$OUT/cc-119" "$OUT/cc-119-diag.json" "$OUT/cc-119.log"
else
  echo "=== Run A' output exists, skipping ==="
fi

echo "=== Run B': humanify v120 with --prior-version ==="
time run_humanify "$V120" "$OUT/cc-120" "$OUT/cc-120-diag.json" "$OUT/cc-120.log" \
  --prior-version "$OUT/cc-119/runtime.js"

# Diff FIRST — it is the deliverable, so nothing downstream can block it.
echo "=== Diff stats (baseline: 167,944 lines / 30,745 hunks) ==="
diff "$OUT/cc-119/runtime.js" "$OUT/cc-120/runtime.js" > "$OUT/runtime-diff.txt" || true
wc -l "$OUT/runtime-diff.txt"
grep -c '^[0-9]' "$OUT/runtime-diff.txt" | xargs echo "hunks:"

# Rename-noise vs genuine-change classification (equal-count change hunks
# that are identical after blanking identifiers are pure rename noise).
echo "=== Noise classification ==="
python3 "$ROOT/experiments/013-bun-cjs-classification/classify-diff.py" \
  "$OUT/runtime-diff.txt" || echo "(classifier unavailable — skipped)"

# Belt-and-braces parse check (the pipeline already validates internally).
# Uses @babel/core parseSync — validateOutputParses was removed as dead code.
echo "=== Parse validation ==="
node --import tsx/esm -e "
  const { parseSync } = await import('@babel/core');
  const fs = await import('node:fs');
  for (const f of ['$OUT/cc-119/runtime.js', '$OUT/cc-120/runtime.js']) {
    try {
      parseSync(fs.readFileSync(f, 'utf-8'), { sourceType: 'unambiguous' });
      console.log(f, 'parses OK');
    } catch (e) {
      console.log(f, 'PARSE FAIL:', e.message);
    }
  }
" || echo "(parse check errored — see above; diff already written)"
