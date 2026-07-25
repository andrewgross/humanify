#!/usr/bin/env bash
# Lever-B measurement for one hop from two REAL full-pipeline trees (both Bun
# re-linked, same prior + LLM cache => identical naming, differ only in emit
# order). Verifies ON is a pure reorder of OFF, boots ON, and reports reorder +
# git churn of each vs the prior tree.
#   leverb-measure.sh <ON_OUTDIR> <OFF_OUTDIR> <PRIOR_SRC_DIR> <PAIR_LABEL>
set -uo pipefail
ON=$1; OFF=$2; PRIOR_SRC=$3; PAIR=$4
REPO=/Users/andrewgross/Development/humanify
HEAP="--max-old-space-size=12288"
rc() { NODE_OPTIONS="$HEAP" npx tsx "$REPO/experiments/037-noise-source-decomposition/reorder-churn.ts" "$1" "$2" 2>/dev/null | grep "REORDER churn lines" | grep -oE "[0-9]+"; }
gc() { git diff --no-index --numstat "$1" "$2" 2>/dev/null | awk '$1!="-"&&$2!="-"{a+=$1;d+=$2} END{print a+d}'; }

echo "=== [$PAIR] verify ON is a PURE reorder of OFF (same naming, diff order) ==="
mism=0; both=0
while IFS= read -r f; do
  rel=${f#"$ON"/src/}; of="$OFF/src/$rel"; [ -f "$of" ] || continue; both=$((both+1))
  diff -q <(sort "$f") <(sort "$of") >/dev/null 2>&1 || mism=$((mism+1))
done < <(find "$ON/src" -name "*.js")
echo "  common files=$both  content-mismatches=$mism  (0 => ON is a pure reorder of OFF)"

echo "=== [$PAIR] BOOT gate (align-ON runnable tree must run) ==="
bv="n/a"
if command -v bun >/dev/null && [ -f "$ON/run.cjs" ]; then
  bv=$( (cd "$ON" && timeout 60 bun run.cjs --version 2>&1 | tail -1) || true )
fi
echo "  bun run.cjs --version => $bv"

echo "=== [$PAIR] churn vs prior ==="
on_r=$(rc "$PRIOR_SRC" "$ON/src"); off_r=$(rc "$PRIOR_SRC" "$OFF/src")
on_g=$(gc "$PRIOR_SRC" "$ON/src"); off_g=$(gc "$PRIOR_SRC" "$OFF/src")
printf '  %-12s OFF=%-9s ON=%-9s  reduction=%s (%.0f%%)\n' "reorderChurn" "$off_r" "$on_r" "$((off_r - on_r))" "$(echo "scale=4;($off_r-$on_r)/$off_r*100"|bc)"
printf '  %-12s OFF=%-9s ON=%-9s  reduction=%s (%.0f%%)\n' "gitChurn" "$off_g" "$on_g" "$((off_g - on_g))" "$(echo "scale=4;($off_g-$on_g)/$off_g*100"|bc)"
# machine-readable row for the table
echo "ROW|$PAIR|$bv|$off_r|$on_r|$off_g|$on_g|$mism"
