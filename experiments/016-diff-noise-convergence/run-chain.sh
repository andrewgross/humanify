#!/usr/bin/env bash
# Shared-lineage diff for exp016: the production scenario the goal
# describes, built from the two decompiled versions we have.
#
# The exp013/014/015 A/B (fresh v119 vs incremental v120) measures
# FIRST-CONTACT noise: the fresh leg re-invents names every run, so
# one-time naming choices (collision suffixes like error→errorVal,
# synonym picks, ordinals) count as recurring noise. In production each
# release is humanified incrementally on the previous one, so ADJACENT
# RELEASES SHARE A NAMING LINEAGE and those choices are inherited, not
# re-made.
#
# With only v119/v120 decompiled, the lineage pair is built by closing
# the loop: an existing incremental v120 output (prior = fresh v119)
# becomes the prior for RE-humanifying v119. Both diffed legs then
# derive from one lineage, exactly like adjacent production releases —
# the direction of one hop is reversed, which only affects code that
# exists in one version (counted as genuine change either way).
#
#   PRIOR_V120 (default /tmp/exp016-r1/cc-120/runtime.js)
#      └─ prior for → leg: humanify v119 → $OUT/cc-119-lineage
#   diff(cc-119-lineage, PRIOR_V120) = the reviewable release diff.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
INPUTS="/Users/andrewgross/Development/claude-code-versions/inputs"
V119="$INPUTS/claude-code-2.1.119/binary-decompiled/src/entrypoints/index.js"
PRIOR_V120="${PRIOR_V120:-/tmp/exp016-r1/cc-120/runtime.js}"
OUT="${CHAIN_OUT:-/tmp/exp016-chain}"

ENDPOINT="${HUMANIFY_ENDPOINT:-http://192.168.1.234:8000/v1}"
MODEL="${HUMANIFY_MODEL:-openai/gpt-oss-20b}"
REASONING_EFFORT="${HUMANIFY_REASONING_EFFORT:-low}"
export HUMANIFY_API_KEY="${HUMANIFY_API_KEY:-local}"
export HUMANIFY_CONCURRENCY="${HUMANIFY_CONCURRENCY:-120}"
export HUMANIFY_MODULE_CONCURRENCY="${HUMANIFY_MODULE_CONCURRENCY:-40}"
export HUMANIFY_MAX_TOKENS="${HUMANIFY_MAX_TOKENS:-2000}"

if [ ! -f "$PRIOR_V120" ]; then
  echo "prior leg missing: $PRIOR_V120 (run run-phase2.sh first)" >&2
  exit 1
fi

mkdir -p "$OUT"

echo "=== Lineage leg: humanify v119 with --prior-version <v120 incremental> ==="
# EXTRA_HUMANIFY_FLAGS: optional additional CLI flags (e.g. exp020's
# --reconcile-prior-diff), word-split on purpose.
# shellcheck disable=SC2086
time node --max-old-space-size=16384 --import tsx/esm "$ROOT/src/index.ts" \
  "$V119" -o "$OUT/cc-119-lineage" \
  --diagnostics "$OUT/cc-119-lineage-diag.json" \
  --endpoint "$ENDPOINT" --model "$MODEL" \
  --reasoning-effort "$REASONING_EFFORT" \
  --bundler bun --minifier bun \
  --log-file "$OUT/cc-119-lineage.log" \
  --prior-version "$PRIOR_V120" \
  ${EXTRA_HUMANIFY_FLAGS:-}

echo "=== Shared-lineage diff: v119-lineage vs v120-incremental ==="
diff "$OUT/cc-119-lineage/runtime.js" "$PRIOR_V120" > "$OUT/runtime-diff.txt" || true
wc -l "$OUT/runtime-diff.txt"
grep -c '^[0-9]' "$OUT/runtime-diff.txt" | xargs echo "hunks:"

echo "=== Noise classification ==="
python3 "$ROOT/experiments/013-bun-cjs-classification/classify-diff.py" \
  "$OUT/runtime-diff.txt" || echo "(classifier unavailable)"

echo "=== Parse validation ==="
node --import tsx/esm -e "
  const { parseSync } = await import('@babel/core');
  const fs = await import('node:fs');
  try {
    parseSync(fs.readFileSync('$OUT/cc-119-lineage/runtime.js', 'utf-8'), { sourceType: 'unambiguous' });
    console.log('cc-119-lineage parses OK');
  } catch (e) {
    console.log('PARSE FAIL:', e.message);
  }
"
