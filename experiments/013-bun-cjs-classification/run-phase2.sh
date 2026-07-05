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
export HUMANIFY_API_KEY="${HUMANIFY_API_KEY:-local}"

mkdir -p "$OUT"

run_humanify() {
  local input="$1" outdir="$2" diag="$3" log="$4"; shift 4
  node --max-old-space-size=16384 --import tsx/esm "$ROOT/src/index.ts" \
    "$input" -o "$outdir" \
    --diagnostics "$diag" \
    --endpoint "$ENDPOINT" --model "$MODEL" \
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

echo "=== Parse validation (belt and braces; the pipeline validates too) ==="
node --import tsx/esm -e "
  const { validateOutputParses } = await import('$ROOT/src/output-validation.ts');
  const fs = await import('node:fs');
  for (const f of ['$OUT/cc-119/runtime.js', '$OUT/cc-120/runtime.js']) {
    const failure = validateOutputParses(fs.readFileSync(f, 'utf-8'));
    console.log(f, failure ? 'PARSE FAIL: ' + failure.message : 'parses OK');
  }
"

echo "=== Diff stats (baseline: 167,944 lines / 30,745 hunks) ==="
diff "$OUT/cc-119/runtime.js" "$OUT/cc-120/runtime.js" > "$OUT/runtime-diff.txt" || true
wc -l "$OUT/runtime-diff.txt"
grep -c '^[0-9]' "$OUT/runtime-diff.txt" | xargs echo "hunks:"
