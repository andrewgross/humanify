#!/usr/bin/env bash
#
# Eval harness driver — score the CURRENT pipeline on the configured version
# pairs and store the result under a MODEL label so runs stack up side by side.
#
#   experiments/034-eval-harness/run.sh <model-label> [workdir]
#
# <model-label> names this run (a branch, a commit, an idea — e.g.
# "main-4117212" or "fix-close-match"). Results land in results/<model-label>/;
# `leaderboard.ts` then compares every model. Re-running a label overwrites it.
#
# One pipeline run per pair (~10-15 min each); a failed pair is logged and
# skipped, never aborts the sweep. Deterministic metrics are stable run-to-run;
# the naming-noise magnitude carries the LLM floor (see README).
set -uo pipefail

MODEL="${1:?usage: run.sh <model-label> [workdir]}"
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
WORK="${2:-/tmp/eval-work}"
CFG="${EVAL_PAIRS:-$HERE/pairs.json}"
RESULTS="$HERE/results/$MODEL"
mkdir -p "$RESULTS" "$WORK"

command -v jq >/dev/null || { echo "jq required"; exit 1; }

# pairs.json carries the laptop's absolute paths, and the bahadur devcontainer
# deliberately mirrors them, so the fixture roots resolve unchanged in both
# places. These env vars cover anywhere that does NOT mirror them -- and the
# endpoint, which genuinely differs (host.docker.internal inside the container).
INPUTS="${EVAL_INPUTS_BASE:-$(jq -r .inputsBase "$CFG")}"
PRIORS="${EVAL_PRIORS_BASE:-$(jq -r .priorsBase "$CFG")}"

# LLM response cache: OFF by default, because it MASKS the model's inherent
# variance. A gate run has to reproduce what a real user sees -- every prompt
# live, from scratch -- and exp047's first gate was accidentally cache-pinned:
# all 24,079 entries pre-dated the run and not one new entry was written, so no
# prompt reached the model at all. The KPIs it produced were replayed answers.
#
# Set EVAL_LLM_CACHE=<dir> to opt IN, for fast iteration or for probing a
# deterministic surface that does not depend on LLM output. Anything whose
# numbers go into a RESULTS.md or a ship/no-ship gate must run without it.
if [[ -n "${EVAL_LLM_CACHE:-}" ]]; then
  LLM_CACHE_ARGS=(--llm-cache "$EVAL_LLM_CACHE")
  echo "LLM CACHE: ON ($EVAL_LLM_CACHE) -- NOT valid for a gate run"
else
  LLM_CACHE_ARGS=()
  # `unified.ts` falls back to HUMANIFY_LLM_CACHE when the flag is absent, so
  # omitting the flag is not enough -- an ambient env var would silently re-pin
  # the run to the cache, which is the exact failure this default exists to
  # prevent.
  unset HUMANIFY_LLM_CACHE
  echo "LLM CACHE: OFF (cold, every prompt live) -- gate-valid"
fi
ENDPOINT="${EVAL_ENDPOINT:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")

# Record what produced this model, for provenance.
git -C "$REPO" rev-parse --short HEAD > "$RESULTS/commit.txt" 2>/dev/null || true

npairs=$(jq '.pairs | length' "$CFG")
for i in $(seq 0 $((npairs - 1))); do
  FROM=$(jq -r ".pairs[$i].from" "$CFG")
  TO=$(jq -r ".pairs[$i].to" "$CFG")
  PAIR="$FROM->$TO"

  # EVAL_PAIRS="215->216,85->86" restricts the sweep to the named pairs, so a
  # targeted probe costs one run instead of the full ~1hr sweep. Either the full
  # form ("2.1.215->2.1.216") or the patch-only shorthand ("215->216") matches.
  if [[ -n "${EVAL_PAIRS:-}" ]]; then
    SHORT="${FROM##*.}->${TO##*.}"
    if [[ ",$EVAL_PAIRS," != *",$PAIR,"* && ",$EVAL_PAIRS," != *",$SHORT,"* ]]; then
      echo "SKIP $PAIR (not in EVAL_PAIRS)"
      continue
    fi
  fi

  INPUT="$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"
  PRIOR="$PRIORS/claude-code-$FROM/.humanify/humanified.js"
  PRIOR_LEDGER="$PRIORS/claude-code-$FROM/.humanify/split-ledger.json"
  OUT="$WORK/$MODEL/$TO"
  STATS="$RESULTS/$TO.stats.json"
  LOG="$RESULTS/$TO.log"

  if [[ ! -f "$INPUT" ]]; then echo "SKIP $PAIR (no input $INPUT)"; continue; fi
  if [[ ! -f "$PRIOR" ]]; then echo "SKIP $PAIR (no prior $PRIOR)"; continue; fi

  # REBASE_PRIOR=1: a formatting change made the archive v-1 an invalid base
  # (formatting diffs would swamp the naming signal). Re-humanify the base with
  # the CURRENT pipeline (inheriting its own archive names) so the pair's diff
  # reflects naming/real change only. Costs one extra run per pair.
  if [[ "${REBASE_PRIOR:-0}" == "1" ]]; then
    INPUT_FROM="$INPUTS/claude-code-$FROM/binary-decompiled/src/entrypoints/index.js"
    if [[ -f "$INPUT_FROM" ]]; then
      REBASE="$WORK/$MODEL/${FROM}-rebased"
      echo "=== $PAIR: rebasing prior (re-humanify $FROM, current pipeline) ==="
      rm -rf "$REBASE"
      NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT_FROM" \
        --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
        --reasoning-effort "$EFFORT" -c "$CONC" -o "$REBASE" \
        "${LLM_CACHE_ARGS[@]+"${LLM_CACHE_ARGS[@]}"}" ${EVAL_NO_WAVE:+--no-wave-scheduling} \
        --prior-version "$PRIOR" -vv --log-file "$RESULTS/${FROM}-rebase.log" \
        > "$RESULTS/${FROM}-rebase.stdout" 2>&1
      if [[ -f "$REBASE/.humanify/humanified.js" ]]; then
        PRIOR="$REBASE/.humanify/humanified.js"
        PRIOR_LEDGER="$REBASE/.humanify/split-ledger.json"
        echo "  prior rebased -> $PRIOR"
      else
        echo "  rebase FAILED; falling back to archive prior"
      fi
    fi
  fi

  echo "=== [$((i + 1))/$npairs] $PAIR: pipeline ==="
  rm -rf "$OUT"
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    "${LLM_CACHE_ARGS[@]+"${LLM_CACHE_ARGS[@]}"}" ${EVAL_NO_WAVE:+--no-wave-scheduling} \
    --prior-version "$PRIOR" --stats-json "$STATS" -vv --log-file "$LOG" \
    --diagnostics "$WORK/$MODEL/$TO.diag.json" \
    > "$RESULTS/$TO.stdout" 2>&1
  if [[ ! -f "$OUT/.humanify/humanified.js" ]]; then
    echo "PIPELINE FAILED for $PAIR (see $RESULTS/$TO.stdout)"; continue
  fi

  echo "=== $PAIR: churn analysis ==="
  # The statement-level churn above is position-BLIND: a byte-identical
  # statement emitted somewhere else costs nothing there and everything in
  # review (how Lever B v1's 118->119 regression hid). Passing both split trees
  # adds the `layout` block — real/naming/alias/REORDER in git lines. Costs a
  # few minutes per pair; EVAL_LAYOUT=0 skips it.
  PRIOR_SRC="$(dirname "$(dirname "$PRIOR")")/src"
  LAYOUT_ARGS=()
  if [[ "${EVAL_LAYOUT:-1}" == "1" && -d "$OUT/src" && -d "$PRIOR_SRC" ]]; then
    LAYOUT_ARGS=("$OUT/src" "$PRIOR_SRC")
    # vendor/ is its OWN scored surface, never folded into the src numbers
    # above (exp046). It was unscored until then, so every reference committed
    # before it prints `-` for the vendor columns -- which is not 0. Vendor
    # scoring rides on LAYOUT_ARGS because analyze.ts takes the trees
    # positionally; EVAL_VENDOR=0 skips it.
    PRIOR_VENDOR="$(dirname "$(dirname "$PRIOR")")/vendor"
    if [[ "${EVAL_VENDOR:-1}" == "1" && -d "$OUT/vendor" && -d "$PRIOR_VENDOR" ]]; then
      LAYOUT_ARGS+=("$OUT/vendor" "$PRIOR_VENDOR")
    fi
  fi
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$HERE/analyze.ts" \
    "$OUT/.humanify/humanified.js" "$PRIOR" \
    "$OUT/.humanify/split-ledger.json" "$PRIOR_LEDGER" \
    "$STATS" "$PAIR" ${LAYOUT_ARGS[@]+"${LAYOUT_ARGS[@]}"} > "$RESULTS/$TO.json" \
    || echo "ANALYZE FAILED for $PAIR"

  # Human-readable evidence page (identifier + diff ledgers, funnel):
  # small HTML committed with the results; the big diag JSON stays in WORK.
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$HERE/trail-report.ts" \
    "$WORK/$MODEL/$TO.diag.json" "$RESULTS/$TO-report.html" \
    "$OUT/.humanify/humanified.js" "$PRIOR" \
    "$OUT/.humanify/split-ledger.json" "$PRIOR_LEDGER" \
    > /dev/null 2>&1 || echo "REPORT PAGE FAILED for $PAIR"

  # Boot gate: an output that does not RUN is invalid no matter what the
  # noise KPIs say. `--version` must echo the version; the live `-p`
  # round-trip (EVAL_BOOT_PROMPT=0 skips) exercises the loader
  # end-to-end. Loud on failure, never aborts the sweep; the verdict
  # lands in <TO>-boot.json next to the pair's stats.
  if command -v bun >/dev/null && [[ -f "$OUT/run.cjs" ]]; then
    BOOT_VERSION=$( (cd "$OUT" && timeout 60 bun run.cjs --version 2>&1 | tail -1) || true )
    BOOT_VERSION=${BOOT_VERSION//\"/}
    BOOT_PROMPT="skipped"
    if [[ "${EVAL_BOOT_PROMPT:-1}" == "1" ]]; then
      BOOT_PROMPT=$( (cd "$OUT" && timeout 120 bun run.cjs -p "say exactly: boot-ok" 2>&1 | tail -1) || true )
      BOOT_PROMPT=${BOOT_PROMPT//\"/}
    fi
    BOOT_OK=false
    if [[ "$BOOT_VERSION" == *"$TO"* ]]; then
      if [[ "${EVAL_BOOT_PROMPT:-1}" != "1" || "$BOOT_PROMPT" == *"boot-ok"* ]]; then
        BOOT_OK=true
      fi
    fi
    printf '{"boot":{"version":"%s","prompt":"%s","ok":%s}}\n' \
      "$BOOT_VERSION" "$BOOT_PROMPT" "$BOOT_OK" > "$RESULTS/$TO-boot.json"
    if [[ "$BOOT_OK" == "true" ]]; then
      echo "BOOT GATE: OK ($BOOT_VERSION)"
    else
      echo "BOOT GATE FAILED for $PAIR: version='$BOOT_VERSION' prompt='$BOOT_PROMPT'"
    fi
  else
    echo "BOOT GATE SKIPPED for $PAIR (no bun or no run.cjs)"
  fi
done

# Self-hop idempotence invariant (SELF_HOP=0 skips): re-humanify the last
# pair's TO version using its own fresh output as --prior-version. Same
# code on both sides means every statement is a hash-twin and every
# function exact-matches, so the pipeline must reproduce its output
# BYTE-IDENTICALLY (bundle and split ledger). Any diff line is
# nondeterminism or a phase-ordering bug — measured 2026-07-23: 99.98%
# of bindings settle mechanically, the ~5 LLM-residue draws are pinned by
# the shared cache (the main leg populates it, so the invariant is
# stable even from a cold cache). Violations are logged loudly but never
# abort the sweep.
if [[ "${SELF_HOP:-1}" == "1" && -f "$WORK/$MODEL/$TO/.humanify/humanified.js" ]]; then
  SELF_BASE="$WORK/$MODEL/$TO"
  SELF_OUT="$WORK/$MODEL/${TO}-selfhop"
  echo "=== self-hop invariant: $TO vs its own output ==="
  rm -rf "$SELF_OUT"
  NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$INPUT" \
    --split --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$SELF_OUT" \
    "${LLM_CACHE_ARGS[@]+"${LLM_CACHE_ARGS[@]}"}" ${EVAL_NO_WAVE:+--no-wave-scheduling} \
    --prior-version "$SELF_BASE/.humanify/humanified.js" \
    > "$RESULTS/$TO-selfhop.stdout" 2>&1
  SELF_OK=true
  SELF_DIFF=0
  if ! cmp -s "$SELF_BASE/.humanify/humanified.js" "$SELF_OUT/.humanify/humanified.js"; then
    SELF_OK=false
    SELF_DIFF=$(diff "$SELF_BASE/.humanify/humanified.js" "$SELF_OUT/.humanify/humanified.js" | wc -l | tr -d ' ')
  fi
  if ! cmp -s "$SELF_BASE/.humanify/split-ledger.json" "$SELF_OUT/.humanify/split-ledger.json"; then
    SELF_OK=false
  fi
  printf '{"selfHop":{"version":"%s","identical":%s,"diffLines":%s}}\n' \
    "$TO" "$SELF_OK" "$SELF_DIFF" > "$RESULTS/self-hop.json"
  if [[ "$SELF_OK" == "true" ]]; then
    echo "SELF-HOP INVARIANT: OK — byte-identical bundle and ledger"
  else
    echo "SELF-HOP INVARIANT VIOLATED: $SELF_DIFF diff lines (see $RESULTS/$TO-selfhop.stdout)"
  fi
fi

echo "=== summarizing model '$MODEL' ==="
npx tsx "$HERE/summarize.ts" "$MODEL"
