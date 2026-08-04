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
# Arg 3 is a scratch dir AND, by default, the root the priors are read from
# ($WORK/exp050-cold). Passing a fresh dir therefore fails twice: once because
# nothing creates it, and once because the priors are not in it. Both cost a
# wasted launch on 2026-08-04. Create it here; the prior lookup below explains
# the override.
mkdir -p "$WORK"

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
  # The common cause is not a missing prior — it is arg 3 being read as a
  # scratch dir when it is ALSO the prior root. Say so, rather than let the
  # message send the reader looking for a bundle that was never there.
  [[ -d "$PRIOR_ROOT" ]] || {
    echo "       $PRIOR_ROOT does not exist at all." >&2
    echo "       Arg 3 (workdir) is also the prior root. To use a separate" >&2
    echo "       workdir — e.g. to run two pairs at once — keep the priors" >&2
    echo "       where they are:" >&2
    echo "         NEUTRALITY_PRIORS=/work/exp050-cold $0 $BASELINE $PAIR $WORK" >&2
  }
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

# Clear last run's exit codes BEFORE anything else. They are read at verdict
# time, and nothing used to delete them — so a leg that died without writing
# one (killed, OOM, the script edited mid-flight) made the verdict silently
# report the PREVIOUS run's exit code for a comparison that never happened.
# That is the same failure this harness exists to catch elsewhere: reading a
# number without first establishing it belongs to this run.
rm -f "$WORK"/neutrality-candidate.rc "$WORK"/neutrality-baseline.rc

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

# A MISSING .rc is fatal, never defaulted. `${VAR:-0}` here would invent a
# clean exit for a leg that never reported one.
for leg in candidate baseline; do
  [[ -f "$WORK/neutrality-$leg.rc" ]] || {
    echo "FATAL: leg $leg recorded no exit code — the verdict would be about" >&2
    echo "       a run that did not happen." >&2
    exit 1
  }
done
RC_A=$(cat "$WORK/neutrality-candidate.rc")
RC_B=$(cat "$WORK/neutrality-baseline.rc")
echo
echo "=== exit codes ==="
echo "  leg candidate: $RC_A"
echo "  leg baseline : $RC_B  <-- must MATCH the candidate"
if [[ "$RC_A" != "0" && "$RC_A" == "$RC_B" ]]; then
  echo "  both legs failed identically — a PRE-EXISTING failure, not this change."
fi

# Diagnostics-only artifacts: written for a reader, consumed by nothing. A new
# one appearing is EXPECTED when a change adds it, and reading that as "the
# emitted code changed" is a false alarm that costs a debugging cycle — it cost
# me one. Everything else under .humanify/ is NOT in this list on purpose:
# `humanified.js` and `split-ledger.json` become the NEXT release's prior, so a
# difference there is as load-bearing as a difference in src/.
DIAGNOSTIC_ONLY=(".humanify/placement-stats.json")

EXCLUDES=()
for f in "${DIAGNOSTIC_ONLY[@]}"; do
  EXCLUDES+=(--exclude="$(basename "$f")")
done

echo
echo "=== tree diff ==="
DIFFLINES=$(diff -rN ${EXCLUDES[@]+"${EXCLUDES[@]}"} \
  "$WORK/neutrality-baseline" "$WORK/neutrality-candidate" 2>/dev/null \
  | grep -cE '^[<>]')
FILESDIFF=$(diff -rq ${EXCLUDES[@]+"${EXCLUDES[@]}"} \
  "$WORK/neutrality-baseline" "$WORK/neutrality-candidate" 2>/dev/null \
  | wc -l | tr -d ' ')
echo "  differing files: $FILESDIFF   (load-bearing output; diagnostics-only artifacts excluded)"
echo "  differing lines: $DIFFLINES"

# Reported, never fatal: a reader still wants to know one appeared.
#
# Iterates the WHOLE array. It used to inspect only ${DIAGNOSTIC_ONLY[0]}, which
# was harmless with one entry and silently wrong with two: every artifact in this
# list is EXCLUDED FROM THE VERDICT, so an unreported one is a difference that
# neither fails the gate nor appears anywhere a reader would see it. Excluding
# something from a gate is only safe while the exclusion is visible.
DIAG_HITS=$(diff -rq "$WORK/neutrality-baseline" "$WORK/neutrality-candidate" 2>/dev/null \
  | grep -F -f <(printf '%s\n' "${DIAGNOSTIC_ONLY[@]}" | xargs -n1 basename) || true)
if [[ -n "$DIAG_HITS" ]]; then
  echo "  note: $(printf '%s\n' "$DIAG_HITS" | grep -c .) diagnostics-only artifact(s) differ — expected when a change adds one:"
  printf '%s\n' "$DIAG_HITS" | sed 's/^/    /'
fi

VERDICT=0
[[ "$DIFFLINES" != "0" ]] && VERDICT=1
[[ $((AFTER_B - AFTER_A)) -ne 0 ]] && VERDICT=1
[[ "$RC_A" != "$RC_B" ]] && VERDICT=1

echo
if [[ $VERDICT -eq 0 ]]; then
  # Report the COUNTS, never a blanket "zero new prompts". The candidate leg
  # populates the shared cache, so its count is routinely non-zero — a run on
  # 2026-08-03 printed "zero new prompts" above a candidate leg that had written
  # 7 entries. The verdict was still sound (only the BASELINE leg's zero is
  # load-bearing: it proves leg B asked nothing leg A had not), but a summary
  # that contradicts the evidence three lines above it is how a harness starts
  # lying quietly — the same failure family as rule 10.
  echo "NEUTRAL: identical bytes, identical exit code ($RC_A) on $FROM->$TO."
  echo "         Cache: candidate +$((AFTER_A - BEFORE_A)), baseline +$((AFTER_B - AFTER_A))"
  echo "         (baseline +0 is the load-bearing one: leg B replayed leg A's answers"
  echo "         exactly, so the two legs asked the same questions)."
  echo "         Scope: this pair, draws pinned. Not a claim about other inputs."
  if [[ "$RC_A" != "0" ]]; then
    echo "         NOTE: both legs exited $RC_A. This change is neutral with respect to a"
    echo "         failure that ALREADY EXISTS on the baseline — it is not a clean run."
  fi
else
  echo "NOT NEUTRAL: this change alters emitted output, the prompts asked, or the exit code."
  diff -rq ${EXCLUDES[@]+"${EXCLUDES[@]}"} \
    "$WORK/neutrality-baseline" "$WORK/neutrality-candidate" 2>/dev/null | head -20
fi

git worktree remove --force "$WT" 2>/dev/null
exit $VERDICT
