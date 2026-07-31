#!/usr/bin/env bash
#
# exp056 — a contiguous multi-hop walk on the CURRENT pipeline, measured per hop.
#
#   experiments/056-multi-hop-walk/walk.sh [workdir]
#
# WHY. Every number in 034-055 is ONE hop. The post-split reconcile and its
# bundle carry are multi-hop mechanisms by construction — the carry's entire
# purpose is that hop N+1 inherits what hop N restored — and a single-hop gate
# cannot see whether that holds, compounds, or decays. Every gate in the arc
# said so explicitly and could not test it.
#
# WHAT IT DOES. With EXP056_COLD=1 it builds the SEED version from scratch with
# no --prior-version at all — a true cold start, where the LLM names the folders
# and files and no layout is inherited — and then walks forward, each hop taking
# the previous hop's own output as --prior-version. That is exactly the shape of
# a production run: one cold release, then a walk.
#
# Without EXP056_COLD it seeds from a tree built by a PREVIOUS pipeline, which
# makes the first hop a format transition rather than a version bump and is
# almost never what you want: measured at 73,027 tree lines against ~1,400 for a
# real hop, and it starves the reconcile because the corpus gate abstains below
# 50% line alignment. `REBASE_PRIOR` in the eval harness exists for this reason.
#
# Per hop it records:
#
#   treeLn      lines `diff -r` prints between this hop's src/ and the prior's
#   renames     what the post-split reconcile applied
#   carried     what reached the bundle for the next hop
#   boot        does the emitted tree run
#
# READING IT. Under EXP056_COLD the seed row is the cold build (no prior, so no
# treeLn) and EVERY hop after it is steady state — that is the point of the flag.
# Without it, hop 1 is a format transition and only hops 2..N mean anything.
#
# NOT DESTRUCTIVE: writes to its own workdir. The live artifact
# (unpacked-claude-code/versions and claude-code-history.git) is untouched.
set -uo pipefail

WORK="${1:-/work/exp056-walk}"
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
SEED="${EXP056_SEED:-2.1.212}"
HOPS="${EXP056_HOPS:-2.1.213 2.1.214 2.1.215 2.1.216}"
VERSIONS="/Users/andrewgross/Development/unpacked-claude-code/versions"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
INPUTS="${EVAL_INPUTS_BASE:-$(jq -r .inputsBase "$CFG")}"
ENDPOINT="${EVAL_ENDPOINT:-$(jq -r .llm.endpoint "$CFG")}"
MODEL=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")
# The eval harness caps the heap at 14GB because it runs four pairs back to
# back. A COLD build has no prior, so every one of ~63,000 functions reaches the
# LLM and the naming state is all live at once — 2.1.212 OOM'd at 14GB, 33% in.
# This box has 251GB; size for the cold build, not for the hops.
HEAP="${EXP056_HEAP:-98304}"

# bun is NOT on PATH here and its absence is SILENT in run.sh — the boot column
# would read "skipped" with no error.
export PATH="$HOME/.bun/bin:$PATH"
command -v bun >/dev/null || echo "WARNING: no bun — the boot column will be blank"

mkdir -p "$WORK/logs"

# One pipeline run. $3 empty => cold start (no --prior-version).
run_hop() {
  local IN="$1" OUT="$2" PRIOR="$3" LOG="$4"
  local ARGS=(--split --endpoint "$ENDPOINT" --model "$MODEL" --api-key "$APIKEY"
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT")
  [ -n "$PRIOR" ] && ARGS+=(--prior-version "$PRIOR")
  rm -rf "$OUT"
  NODE_OPTIONS="--max-old-space-size=$HEAP" npx tsx "$REPO/src/index.ts" "$IN" \
    "${ARGS[@]}" -vv --log-file "$LOG" > "${LOG%.log}.stdout" 2>&1
  [ -f "$OUT/.humanify/humanified.js" ]
}

PRIOR_DIR="$WORK/$SEED"
if [ ! -f "$PRIOR_DIR/.humanify/humanified.js" ]; then
  if [ "${EXP056_COLD:-0}" = "1" ]; then
    echo "COLD START: building $SEED from scratch, no prior — the LLM names the tree"
    if ! run_hop "$INPUTS/claude-code-$SEED/binary-decompiled/src/entrypoints/index.js" \
        "$PRIOR_DIR" "" "$WORK/logs/$SEED.log"; then
      echo "$SEED: COLD START FAILED (see $WORK/logs/$SEED.stdout)"; exit 1
    fi
  else
    echo "seeding $SEED from the previous pipeline's walk output"
    rm -rf "$PRIOR_DIR"
    cp -a "$VERSIONS/claude-code-$SEED" "$PRIOR_DIR" || exit 1
  fi
fi

printf '%-10s %10s %10s %10s %8s %6s\n' VERSION treeLn renames carried abstain boot
for V in $HOPS; do
  IN="$INPUTS/claude-code-$V/binary-decompiled/src/entrypoints/index.js"
  OUT="$WORK/$V"
  LOG="$WORK/logs/$V.log"
  if [ ! -f "$IN" ]; then echo "$V: NO INPUT — stopping"; break; fi
  if [ ! -f "$OUT/.humanify/humanified.js" ]; then
    if ! run_hop "$IN" "$OUT" "$PRIOR_DIR/.humanify/humanified.js" "$LOG"; then
      # A failed hop STOPS the walk: the next hop's prior would be missing and
      # we must never build on a hole.
      echo "$V: PIPELINE FAILED (see $WORK/logs/$V.stdout) — stopping"
      break
    fi
  fi
  TREELN=$(diff -r "$PRIOR_DIR/src" "$OUT/src" 2>/dev/null | grep -cE '^[<>]')
  LINE=$(grep -o "restored [0-9]* prior name" "$WORK/logs/$V.stdout" | grep -o '[0-9]*' | head -1)
  CARRY=$(grep -o "carried [0-9]*/[0-9]* name" "$WORK/logs/$V.stdout" | head -1 | grep -o '^carried [0-9]*' | grep -o '[0-9]*')
  ABST=$(grep -o "([0-9]* abstained)" "$WORK/logs/$V.stdout" | head -1 | grep -o '[0-9]*')
  BOOT="-"
  if command -v bun >/dev/null && [ -f "$OUT/run.cjs" ]; then
    BV=$( (cd "$OUT" && timeout 60 bun run.cjs --version 2>&1 | tail -1) || true )
    case "$BV" in *"$V"*) BOOT=ok ;; *) BOOT=FAIL ;; esac
  fi
  printf '%-10s %10s %10s %10s %8s %6s\n' \
    "$V" "$TREELN" "${LINE:-0}" "${CARRY:-0}" "${ABST:-0}" "$BOOT"
  PRIOR_DIR="$OUT"
done
echo "WALK COMPLETE"
