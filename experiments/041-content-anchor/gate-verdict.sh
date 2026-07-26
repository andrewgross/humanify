#!/usr/bin/env bash
#
# The exp041 gate, judged in one pass. Every check is non-negotiable and every
# hop is judged ON ITS OWN — a big hop masks a regression on a small one, which
# is how Lever B v1's 118->119 regression hid.
#
#   experiments/041-content-anchor/gate-verdict.sh [control] [candidate]
#
# Defaults compare the exp041 candidate against its same-host control.
set -uo pipefail
cd "$(dirname "$0")/../.."

CONTROL="${1:-exp041-base}"
CAND="${2:-exp041-anchor}"
RESULTS="experiments/034-eval-harness/results"
W=/work

echo "############ 1. RELOCATION per hop (the success criterion) ############"
echo "   down on EVERY hop, or the change does not ship."
for pair in "2.1.85-rebased 2.1.86 85->86" \
            "2.1.118-rebased 2.1.119 118->119" \
            "2.1.197-rebased 2.1.198 197->198" \
            "2.1.215-rebased 2.1.216 215->216"; do
  set -- $pair
  for model in "$CONTROL" "$CAND"; do
    if [[ -d "$W/$model/$2/src" && -d "$W/$model/$1/src" ]]; then
      NODE_OPTIONS="--max-old-space-size=16384" npx tsx \
        experiments/040-diff-census/relocation-churn.ts \
        "$W/$model/$1/src" "$W/$model/$2/src" "$model $3" 2>/dev/null \
        | grep -E "TOTAL relocation" | sed "s#^#  $model $3: #"
    else
      echo "  $model $3: MISSING TREES"
    fi
  done
done

echo
echo "############ 2. KPIs — noise down, REAL CHANGE UNMOVED ############"
echo "   Read the header marks: down-arrow = drive to zero, '=' = REAL CODE"
echo "   CHANGE and must not move in EITHER direction, '~' = a move means"
echo "   nothing on its own. Dropping real change is a regression wearing a"
echo "   win's clothing, and the '=' columns are the only place it shows."
npx tsx experiments/034-eval-harness/leaderboard.ts "$CONTROL" "$CAND" 2>&1 | tail -30

echo
echo "############ 3. BOOT GATE — an output that does not RUN is invalid ############"
for f in "$RESULTS/$CAND"/*-boot.json; do
  [[ -e "$f" ]] || { echo "  NO BOOT RESULTS (bun missing? gate would have SKIPPED)"; break; }
  echo "  $(basename "$f"): $(cat "$f")"
done

echo
echo "############ 4. SELF-HOP — byte-identical in bundle AND ledger ############"
echo "  control:   $(cat "$RESULTS/$CONTROL/self-hop.json" 2>/dev/null)"
echo "  candidate: $(cat "$RESULTS/$CAND/self-hop.json" 2>/dev/null)"

echo
echo "############ 5. DID THE TIERS FIRE WHERE THE CEILING PREDICTED? ############"
echo "   Predicted (anchor / allsame): 85->86 75/170, 118->119 6/31,"
echo "   197->198 60/91, 215->216 49/124. A KPI that moved while the tiers"
echo "   fired somewhere else is a bug, not a win."
if command -v jq >/dev/null; then
  for v in 2.1.86 2.1.119 2.1.198 2.1.216; do
    diag="$W/$CAND/$v.diag.json"
    [[ -f "$diag" ]] || { echo "  $v: no diagnostics"; continue; }
    echo "  $v: $(jq -c '.placementTrails.tiers' "$diag" 2>/dev/null)"
  done
fi
