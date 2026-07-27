#!/usr/bin/env bash
#
# exp048 Task 3 — read the renames the family-permute pass actually shipped.
#
#   experiments/048-family-permute-cold/read-moves.sh <label> [sample]
#
# The pass rewrites names in the FINAL artifact. Its v1 cut renamed a CORRECT
# name (getClaudeCodeOAuthToken -> deviceActionMap) and no metric caught it — a
# human reading the diff did. So the moves get read, not counted.
#
# It is also the attribution instrument for the cold A/B. exp037 measures this
# code at -239 noiseLn on one hop, INSIDE exp047's +/-350 cold draw band, so a
# KPI delta cannot be attributed to the pass by magnitude. This can attribute it
# from the other side: a hop whose move count is 0 cannot have had its KPIs moved
# by the pass, whatever they read.
#
# Reads the `-vv` run logs the eval already writes, so it costs nothing extra.
set -uo pipefail
cd "$(dirname "$0")/../.."

LABEL="${1:?usage: read-moves.sh <label> [sample-size]}"
SAMPLE="${2:-40}"
RESULTS="experiments/034-eval-harness/results/$LABEL"

[[ -d "$RESULTS" ]] || { echo "no results dir: $RESULTS" >&2; exit 1; }

# TOTAL first, then the per-hop breakdown, then the reading sample.
TOTAL=0
declare -a ROWS=()
for LOG in "$RESULTS"/2.1.*.log; do
  [[ -e "$LOG" ]] || continue
  V=$(basename "$LOG" .log)
  N=$(grep -cE '^ +move .* -> .* \(support [0-9]+' "$LOG" 2>/dev/null || echo 0)
  B=$(grep -oE 'bucket [0-9a-f]+' "$LOG" 2>/dev/null | sort -u | wc -l)
  ROWS+=("$(printf '  %-12s %6s moves in %5s bucket(s)' "$V" "$N" "$B")")
  TOTAL=$((TOTAL + N))
done

echo "TOTAL family-permute moves shipped ($LABEL): $TOTAL"
printf '%s\n' "${ROWS[@]+"${ROWS[@]}"}"

echo
echo "Support distribution (how much call-site evidence each move had):"
grep -hoE '\(support [0-9]+' "$RESULTS"/2.1.*.log 2>/dev/null \
  | awk '{print $2}' | sort -n | uniq -c \
  | awk '{printf "  support %-4s %6s move(s)\n", $2, $1}'

echo
echo "Sample of $SAMPLE moves TO READ (from -> to). A move onto a name that is"
echo "already meaningful, or away from one, is the v1 failure — read for that:"
grep -hoE 'move [A-Za-z_$][^ ]* -> [A-Za-z_$][^ ]* \(support [0-9]+' \
  "$RESULTS"/2.1.*.log 2>/dev/null | sed 's/^/  /' | head -"$SAMPLE"
