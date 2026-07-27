#!/usr/bin/env bash
#
# exp048 Task 1 gate — prove the rebased pass, DISABLED, is byte-identical to main.
#
#   experiments/048-family-permute-cold/byte-identity.sh <label> <outdir> [extra env...]
#
# One pipeline run on the 85->86 pair (smallest base) against the archive prior.
# Called twice by the caller, with a `git checkout` in between:
#
#   leg 1  on main                              -> populates the cache
#   leg 2  on the rebased branch, pass OFF      -> replays it
#
# THE CACHE IS DELIBERATE HERE and this is the one place it is legitimate: the
# question is whether two code paths issue the same instructions, and that can
# only be seen with the LLM's own variance pinned (rule 10 permits the cache for
# "probing a deterministic surface" — never for a number). No output of this
# script goes into RESULTS.md as a KPI; it produces a yes/no on byte identity.
#
# Leg ordering matters: leg 1 must run first so every prompt is on disk when leg
# 2 asks for it. A cache MISS in leg 2 would re-draw and show up as a false
# difference, so the caller checks that leg 2 wrote no new entries.
set -uo pipefail

LABEL="${1:?usage: byte-identity.sh <label> <outdir>}"
OUT="${2:?usage: byte-identity.sh <label> <outdir>}"
HERE="$(cd "$(dirname "$0")" && pwd)"
# REPO_OVERRIDE exists because the two legs run from DIFFERENT checkouts of this
# repo: the caller copies this script outside the tree (a `git checkout main`
# would otherwise delete the branch-only script mid-experiment) and points it
# back at the working copy.
REPO="${REPO_OVERRIDE:-$(cd "$HERE/../.." && pwd)}"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
CACHE="${IDENTITY_CACHE:-/work/llm-cache}"

FROM=2.1.85
TO=2.1.86
INPUTS="${EVAL_INPUTS_BASE:-$(jq -r .inputsBase "$CFG")}"
PRIORS="${EVAL_PRIORS_BASE:-$(jq -r .priorsBase "$CFG")}"
INPUT="$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"
PRIOR="$PRIORS/claude-code-$FROM/.humanify/humanified.js"

ENDPOINT="${EVAL_ENDPOINT:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")

echo "[$LABEL] HEAD=$(git -C "$REPO" rev-parse --short HEAD) branch=$(git -C "$REPO" rev-parse --abbrev-ref HEAD)"
echo "[$LABEL] HUMANIFY_NO_FAMILY_PERMUTE='${HUMANIFY_NO_FAMILY_PERMUTE:-}'  out=$OUT"
rm -rf "$OUT"
NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT" \
  --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
  --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
  --llm-cache "$CACHE" --prior-version "$PRIOR" \
  -vv --log-file "$OUT.log" > "$OUT.stdout" 2>&1
echo "[$LABEL] exit=$? bundle=$(wc -c < "$OUT/.humanify/humanified.js" 2>/dev/null || echo MISSING)"
