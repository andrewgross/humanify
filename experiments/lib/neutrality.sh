#!/usr/bin/env bash
#
# Prove a change emits IDENTICAL BYTES — the right gate for a refactor.
#
#   experiments/lib/neutrality.sh <baseline-ref> [pair] [workdir]
#   experiments/lib/neutrality.sh main 2.1.85:2.1.86
#
# WHY THIS EXISTS INSTEAD OF RUNNING THE EVAL. The consolidation arc is mostly
# changes that are supposed to alter nothing a user can see: extracting a
# helper, adding a counter, single-sourcing a default. For those, the four-pair
# eval is the WRONG instrument twice over:
#
#   - Rule 11: the src/ per-hop draw band is +/-2,800 lines. An eval cannot
#     resolve an effect of exactly zero; it will print a confident number and a
#     sign for pure noise, and a reviewer will read that sign as a finding.
#   - It costs ~1h to be less certain than a byte comparison that costs ~10min.
#
# So this asks the question a refactor actually raises — "is the output the
# same?" — and answers it exactly, rather than asking "did the KPIs move?" and
# answering it noisily.
#
# HOW IT ISOLATES. Both legs share ONE warm LLM cache and run the same input,
# prior and flags. The prompts are therefore replayed, not redrawn, so any
# difference in the emitted tree is attributable to the code change. This is
# the use of the cache rule 10 explicitly permits: it forbids the cache for a
# verdict about LLM-DEPENDENT behaviour, and this is a verdict about
# determinism with the LLM held fixed.
#
# THE CACHE-WRITE COUNT IS A LOAD-BEARING DIAGNOSTIC, not a footnote. Leg A
# populates; leg B replays. If leg B writes entries, the two legs asked the
# model DIFFERENT QUESTIONS — which is itself a behaviour change, and one a
# tree diff might not show if the model happened to answer alike. A clean pass
# needs BOTH: zero new prompts and identical bytes.
#
# WHAT IT CANNOT SEE, and this must be stated with any pass it reports:
#   1. Changes that only manifest on a DIFFERENT input than the pair run here.
#   2. Changes to how prompts are BUILT that leave the cache key alone.
#   3. Multi-hop feedback — one hop is scored, not a walk.
# A pass here means "inert on this pair with draws pinned", never "inert".
set -uo pipefail

BASELINE="${1:?usage: neutrality.sh <baseline-ref> [from:to] [workdir]}"
PAIR="${2:-2.1.85:2.1.86}"
WORK="${3:-/work}"

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
cd "$REPO"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
CACHE="${NEUTRALITY_CACHE:-$WORK/neutrality-cache}"
PRIOR_ROOT="${NEUTRALITY_PRIORS:-$WORK/exp050-cold}"

FROM="${PAIR%%:*}"
TO="${PAIR##*:}"

INPUTS="${EVAL_INPUTS_BASE:-$(jq -r .inputsBase "$CFG")}"
ENDPOINT="${EVAL_ENDPOINT:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")
HEAP="${EVAL_HEAP:-65536}"

INPUT="$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"

# `--prior-version` takes the prior BUNDLE, not the prior tree — the tree's
# `.humanify/humanified.js` inside it. Passing the directory fails with a bare
# EISDIR from readFileSync, which is what happened the first time this ran.
# Prefer the rebased tree: `run.sh` defaults to the ARCHIVE prior, and scoring
# against the wrong base reads ~3.7x worse for no reason.
PRIOR_BASE="$PRIOR_ROOT/$FROM-rebased"
[[ -d "$PRIOR_BASE" ]] || PRIOR_BASE="$PRIOR_ROOT/$FROM"
PRIOR="$PRIOR_BASE/.humanify/humanified.js"

[[ -f "$INPUT" ]] || { echo "FATAL: no input at $INPUT" >&2; exit 1; }
[[ -f "$PRIOR" ]] || {
  echo "FATAL: no prior bundle at $PRIOR" >&2
  echo "       (looked under $PRIOR_ROOT for '$FROM-rebased' then '$FROM')" >&2
  exit 1
}

# Refuse to run on a dirty tree: the candidate leg runs the WORKING TREE, so
# uncommitted changes are the thing under test and unstaged ones would be
# silently included in a result attributed to the branch.
if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  echo "FATAL: working tree is dirty. The candidate leg IS the working tree," >&2
  echo "       so a result from here would not correspond to any commit." >&2
  exit 1
fi

CANDIDATE_REF="$(git rev-parse --short HEAD)"
CANDIDATE_NAME="$(git rev-parse --abbrev-ref HEAD)"
BASE_SHA="$(git rev-parse --short "$BASELINE")"

echo "=== neutrality: $CANDIDATE_NAME ($CANDIDATE_REF) vs $BASELINE ($BASE_SHA) on $FROM->$TO ==="
if [[ "$CANDIDATE_REF" == "$BASE_SHA" ]]; then
  echo "FATAL: candidate and baseline are the same commit — nothing to compare." >&2
  exit 1
fi

count_cache() { find "$CACHE" -type f 2>/dev/null | wc -l | tr -d ' '; }

# Run one leg from a given git ref. The BASELINE leg runs in a detached
# worktree so the current checkout is never touched — switching branches under
# a running pipeline is how a previous run got a half-old, half-new tree.
run_leg() {
  # Separate statements deliberately: under `set -u`, a single `local A=.. B=$A`
  # declares BOTH names unset before running either assignment, so the second
  # expansion aborts the script.
  local LABEL="$1"
  local SRCDIR="$2"
  local OUT="$WORK/neutrality-$LABEL"
  rm -rf "$OUT"
  echo "--- leg $LABEL: $SRCDIR"
  NODE_OPTIONS="--max-old-space-size=$HEAP" npx tsx "$SRCDIR/src/index.ts" \
    "$INPUT" --split \
    --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$OUT" \
    --llm-cache "$CACHE" --prior-version "$PRIOR" \
    > "$WORK/neutrality-$LABEL.stdout" 2>&1
  local RC=$?
  # A non-zero exit is NOT automatically fatal here. The pipeline exits 1 when a
  # file fails the rename-invariant check, having written a complete tree — and
  # it does that today on 2 of the 4 eval pairs (see the runtime.js finding).
  # Aborting on that would make this script unusable on exactly the pairs where
  # a refactor most needs checking. What matters for NEUTRALITY is that both
  # legs fail the SAME way and emit the same bytes, so the code is recorded and
  # compared rather than swallowed. A missing tree IS fatal: there is nothing to
  # compare.
  echo "$RC" > "$WORK/neutrality-$LABEL.rc"
  if [[ ! -d "$OUT" ]]; then
    echo "FATAL: leg $LABEL exited $RC and wrote no tree. Tail of $WORK/neutrality-$LABEL.stdout:" >&2
    tail -20 "$WORK/neutrality-$LABEL.stdout" >&2
    return 1
  fi
  if [[ $RC -ne 0 ]]; then
    echo "    note: leg $LABEL exited $RC but wrote a tree — recorded, compared, not swallowed"
    grep -E "^ERROR:" "$WORK/neutrality-$LABEL.stdout" | head -3 | sed 's/^/      /'
  fi
  echo "    -> $OUT"
}

BEFORE_A=$(count_cache)
run_leg candidate "$REPO" || exit 1
AFTER_A=$(count_cache)

WT="$WORK/neutrality-baseline-src"
rm -rf "$WT"
git worktree remove --force "$WT" 2>/dev/null
git worktree add --detach "$WT" "$BASELINE" >/dev/null 2>&1 || {
  echo "FATAL: could not create a worktree at $WT for $BASELINE" >&2; exit 1; }
# The worktree needs the repo's installed deps; symlink rather than reinstall.
ln -sfn "$REPO/node_modules" "$WT/node_modules"

run_leg baseline "$WT" || { git worktree remove --force "$WT"; exit 1; }
AFTER_B=$(count_cache)

echo
echo "=== cache writes ==="
echo "  leg candidate: $((AFTER_A - BEFORE_A)) new entries (populates; any count is fine)"
echo "  leg baseline : $((AFTER_B - AFTER_A)) new entries  <-- MUST be 0"

RC_A=$(cat "$WORK/neutrality-candidate.rc")
RC_B=$(cat "$WORK/neutrality-baseline.rc")
echo
echo "=== exit codes ==="
echo "  leg candidate: $RC_A"
echo "  leg baseline : $RC_B  <-- must MATCH the candidate"
if [[ "$RC_A" != "0" && "$RC_A" == "$RC_B" ]]; then
  echo "  both legs failed identically — a PRE-EXISTING failure, not this change."
fi

echo
echo "=== tree diff ==="
DIFFLINES=$(diff -rN "$WORK/neutrality-baseline" "$WORK/neutrality-candidate" 2>/dev/null | grep -cE '^[<>]')
FILESDIFF=$(diff -rq "$WORK/neutrality-baseline" "$WORK/neutrality-candidate" 2>/dev/null | wc -l | tr -d ' ')
echo "  differing files: $FILESDIFF"
echo "  differing lines: $DIFFLINES"

VERDICT=0
[[ "$DIFFLINES" != "0" ]] && VERDICT=1
[[ $((AFTER_B - AFTER_A)) -ne 0 ]] && VERDICT=1
[[ "$RC_A" != "$RC_B" ]] && VERDICT=1

echo
if [[ $VERDICT -eq 0 ]]; then
  echo "NEUTRAL: identical bytes, identical exit code ($RC_A), zero new prompts on $FROM->$TO."
  echo "         Scope: this pair, draws pinned. Not a claim about other inputs."
  if [[ "$RC_A" != "0" ]]; then
    echo "         NOTE: both legs exited $RC_A. This change is neutral with respect to a"
    echo "         failure that ALREADY EXISTS on the baseline — it is not a clean run."
  fi
else
  echo "NOT NEUTRAL: this change alters emitted output, the prompts asked, or the exit code."
  diff -rq "$WORK/neutrality-baseline" "$WORK/neutrality-candidate" 2>/dev/null | head -20
fi

git worktree remove --force "$WT" 2>/dev/null
exit $VERDICT
