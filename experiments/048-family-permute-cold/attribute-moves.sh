#!/usr/bin/env bash
#
# exp048 — attribute a hop's KPI delta to the pass, or refuse to.
#
#   experiments/048-family-permute-cold/attribute-moves.sh <version> [candidate] [control]
#   e.g.  attribute-moves.sh 2.1.119
#
# The problem this solves: exp037 measures this pass at -239 noiseLn on one hop,
# which is INSIDE the cold draw band, so a leaderboard delta cannot be credited
# to the pass by magnitude. Two legs of a cold A/B differ by tens of thousands of
# lines from LLM draws alone; a KPI can move thousands either way with the pass
# doing nothing (measured here: 85->86 moved -60 noise and +85 real with ZERO
# moves shipped).
#
# So attribute from the mechanism instead. Every move restores one prior name to
# one binding, and a name is tree-wide, so the lines that move CAN only be the
# lines that name occupies. Summing the restored names' occurrences in the
# emitted tree gives a CEILING on the pass's noise reduction — generous, since it
# assumes every occurrence would otherwise have been a churned line.
#
# Compare that ceiling against the measured delta. Ceiling >> delta: the pass may
# explain it. Ceiling << delta: it cannot, and the delta is draw variance no
# matter how good it looks.
#
# NOTE the ceiling does NOT capture second-order effects — a restored name can
# change a split name vote and move a statement to another file. Those are real
# and unbounded by this count (measurement-pitfalls rule 5). Treat a ceiling
# BELOW the delta as evidence the delta is unattributable, not as proof the pass
# did exactly this much.
set -uo pipefail
cd "$(dirname "$0")/../.."

V="${1:?usage: attribute-moves.sh <version> [candidate-label] [control-label]}"
CAND="${2:-exp048-cold}"
CTRL="${3:-exp048-cold-control}"
RESULTS="experiments/034-eval-harness/results"
LOG="$RESULTS/$CAND/$V.log"
TREE="/work/$CAND/$V/src"

[[ -f "$LOG" ]] || { echo "no run log: $LOG" >&2; exit 1; }
[[ -d "$TREE" ]] || { echo "no emitted tree: $TREE" >&2; exit 1; }

MOVES=$(grep -aoE 'move [^ ]+ -> [^ ]+ ' "$LOG" || true)
N=$(printf '%s' "$MOVES" | grep -c . || true)

echo "=== $V: moves the pass SHIPPED: $N ==="
if [[ "$N" == "0" ]]; then
  echo "  The pass applied nothing on this hop. Any KPI delta here is the model,"
  echo "  not the code — attribute nothing to the pass."
  exit 0
fi

TOT=0
while read -r line; do
  [[ -n "$line" ]] || continue
  FROM=$(awk '{print $2}' <<<"$line")
  TO=$(awk '{print $4}' <<<"$line")
  OCC=$(grep -rhow "$TO" "$TREE" 2>/dev/null | wc -l)
  TOT=$((TOT + OCC))
  printf '  %-34s -> %-28s %5s line(s)\n' "$FROM" "$TO" "$OCC"
done <<<"$MOVES"

echo
echo "  CEILING on this hop's noise reduction: $TOT line(s)"
echo
for K in noise naming reorder alias real; do
  A=$(npx tsx -e "try{console.log(require('$PWD/$RESULTS/$CTRL/$V.json').churn.layout.$K)}catch(e){console.log('-')}" 2>/dev/null)
  B=$(npx tsx -e "try{console.log(require('$PWD/$RESULTS/$CAND/$V.json').churn.layout.$K)}catch(e){console.log('-')}" 2>/dev/null)
  printf '  layout.%-8s control %8s  candidate %8s  delta %8s\n' "$K" "$A" "$B" "$((B - A))"
done
echo
echo "  Read the delta against the ceiling. A delta larger than the ceiling is"
echo "  NOT this pass's doing."
