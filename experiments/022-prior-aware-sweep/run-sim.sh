#!/bin/bash
# Exp022 offline sim: measure the prior-aware sweep mechanism with ZERO
# pipeline changes, by composing the exp020/exp021 harnesses.
#
# Chains (all on the same fresh artifacts, so comparisons carry no leg jitter):
#   C — exp021 deterministic baseline: both legs det-floored, reconcile, diff
#   B — exp021 full-floor baseline:   both legs det+sweep,   reconcile, diff
#   A — exp022 prior-aware mechanism: 120 det+sweep (first-release leg);
#       119 det-floored only, reconciled against 120F (asymmetric tier
#       transfers prior names onto minted sweep targets), THEN the LLM sweep
#       runs on the residue only (--sweep-only). This is exactly the
#       reordered pipeline: floor -> generate -> reconcile -> sweep residue.
#
# The chain-A reconcile dump sizes the win (Step 0): asymmetric renames on
# sweep-target-shaped names = the addressable population.
set -euo pipefail
cd "$(dirname "$0")/../.."

OUT=${OUT:-/tmp/e022}
V119=${V119:-/tmp/exp019-chain/cc-119-lineage/runtime.js}
V120=${V120:-/tmp/exp016-r1/cc-120/runtime.js}
export HUMANIFY_CONCURRENCY=${HUMANIFY_CONCURRENCY:-120}
export HUMANIFY_REASONING_EFFORT=${HUMANIFY_REASONING_EFFORT:-low}

mkdir -p "$OUT"

floor() { npx tsx experiments/021-naming-floor/run-floor.ts "$@"; }
recon() { npx tsx experiments/020-tail-polish/run-reconcile.ts "$@"; }
census() { npx tsx experiments/021-naming-floor/census-minted-tokens.ts "$@"; }

step() { echo; echo "=== [$(date +%H:%M:%S)] $* ==="; }

# ---- floored legs ----------------------------------------------------------
step "120D: deterministic floor (no LLM)"
floor "$V120" --out "$OUT/120D.js" | tee "$OUT/120D.log"

step "119D: deterministic floor (no LLM)"
floor "$V119" --out "$OUT/119D.js" | tee "$OUT/119D.log"

step "120F: deterministic floor + LLM sweep (first-release leg)"
floor "$V120" --sweep --out "$OUT/120F.js" | tee "$OUT/120F.log"

step "119Fi: deterministic floor + INDEPENDENT LLM sweep (exp021 protocol)"
floor "$V119" --sweep --out "$OUT/119Fi.js" | tee "$OUT/119Fi.log"

# ---- chain C: deterministic baseline (exp021's 658 noise / 411 census) -----
step "chain C: reconcile 119D against 120D"
recon --new "$OUT/119D.js" --prior "$OUT/120D.js" --descriptive --apply \
  --out "$OUT/chainC-119.js" --dump "$OUT/chainC-renames.json" \
  | tee "$OUT/chainC.log"

# ---- chain B: full-floor baseline (exp021's 709 noise / 211 census) --------
step "chain B: reconcile 119Fi against 120F"
recon --new "$OUT/119Fi.js" --prior "$OUT/120F.js" --descriptive --apply \
  --out "$OUT/chainB-119.js" --dump "$OUT/chainB-renames.json" \
  | tee "$OUT/chainB.log"

# ---- chain A: exp022 prior-aware mechanism ---------------------------------
step "chain A: reconcile 119D against 120F (prior-name transfer)"
recon --new "$OUT/119D.js" --prior "$OUT/120F.js" --descriptive --apply \
  --out "$OUT/chainA-119R.js" --dump "$OUT/chainA-renames.json" \
  | tee "$OUT/chainA-recon.log"

step "chain A: LLM sweep over the residue only"
floor "$OUT/chainA-119R.js" --sweep-only --out "$OUT/chainA-119.js" \
  | tee "$OUT/chainA-sweep.log"

# ---- diffs + metrics --------------------------------------------------------
for c in chainC chainB chainA; do
  prior="$OUT/120F.js"
  [ "$c" = chainC ] && prior="$OUT/120D.js"
  step "$c: diff + attribute-noise + census"
  diff "$OUT/${c}-119.js" "$prior" > "$OUT/${c}-diff.txt" || true
  python3 experiments/014-rename-noise-elimination/attribute-noise.py \
    "$OUT/${c}-diff.txt" 10 > "$OUT/${c}-noise.txt"
  census "$OUT/${c}-119.js" > "$OUT/${c}-census.txt"
done
census "$OUT/120F.js" > "$OUT/120F-census.txt"
census "$OUT/120D.js" > "$OUT/120D-census.txt"

step "DONE"
