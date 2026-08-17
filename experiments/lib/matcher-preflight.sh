#!/usr/bin/env bash
#
# Matcher preflight for the eval: validate the fingerprint matcher against REAL
# npm packages before spending an hour scoring four claude-code pairs.
#
# WHY IT IS HERE AND NOT IN `npm run check`. `test/e2e/harness/index.ts validate`
# reports absolute precision/recall against ground truth, and one fixture cannot
# reach the threshold: zustand's `getState`/`getInitialState` are identical
# `() => variable` shapes with no distinguishing callees, so they stay ambiguous
# by design (a documented limitation, not a defect). A straight pass/fail gate
# would therefore be permanently red, and a permanently red check is one nobody
# reads.
#
# So this asserts the EXPECTED OUTCOME SET instead: the fixtures that pass must
# still pass, and the one that does not must fail for the same reason and no
# worse. A change in the set is the signal; the absolute numbers are context.
#
# The snapshot tests (`test:fingerprint`, in the fast gate) already catch
# regressions on these same fixtures precisely. This adds the human-readable
# ground-truth view and, more importantly, checks that the harness itself still
# works — it had drifted out of every gate and nothing proved it ran.
#
# Usage:  experiments/lib/matcher-preflight.sh          # all ready fixtures
#         experiments/lib/matcher-preflight.sh --skip   # skip (says so loudly)
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"

if [[ "${1:-}" == "--skip" ]]; then
  echo "MATCHER PREFLIGHT: SKIPPED by --skip — the matcher is unverified for this run"
  return 0 2>/dev/null || exit 0
fi

# Fixtures whose validate must SUCCEED, and the ones known to fall short.
# Anything that moves between these two lists is the finding.
#
# zustand JOINED THE PASS LIST 2026-08-17 (exp079 task 1). It had been the
# permanent known shortfall: `getState` and `getInitialState` both minify to
# "a function of no arguments returning one variable" — no calls, no
# literals, no branches — so nothing structural could separate them, and the
# harness scored 71% / 83% against ground truth for as long as it existed.
#
# What fixed it was not a new signal but reading one already present.
# `B = { setState: z, getState: A, getInitialState: () => D }` — the second
# is written directly as a property value and we read its key; the first is
# assigned to a variable and only then used as a property, one hop away, and
# `extractMemberKey` stopped at the hop. Following that single reference
# takes both pairs to 100%, with memberKey resolving 1 and 2 respectively.
#
# The shortfall list is now EMPTY. If a future fixture joins it, say why here
# — a permanently-red check is one nobody reads, which is why this script
# asserts an outcome SET rather than a threshold.
EXPECT_PASS=(mitt nanoid preact zustand)
EXPECT_SHORTFALL=()

# A fixture's `build/` is a GITIGNORED build artifact produced by
# `npx tsx test/e2e/harness/index.ts setup <fixture>`. A fresh git worktree
# — which is exactly what a frozen scored run uses — does not have it, the
# harness then dies on ENOENT, and this script used to read that as
# "REGRESSED: the matcher behaves differently than recorded". It is not a
# matcher finding at all; it means the check COULD NOT RUN. Both frozen-tree
# validation runs to date (exp074-r1, exp076-r1) reported three false
# regressions this way while the matcher went genuinely unverified and
# nothing said so. Distinguishing the two is the whole point of a preflight:
# a check that cries wolf is spent before it is needed.
fixture_unbuilt() {
  local f="$1"
  compgen -G "$REPO/test/e2e/fixtures/$f/build/*/build" > /dev/null && return 1
  return 0
}

echo "=== matcher preflight: real-package fingerprint validation ==="
PREFLIGHT_BAD=0
PREFLIGHT_UNBUILT=0

for f in "${EXPECT_PASS[@]}" "${EXPECT_SHORTFALL[@]}"; do
  if fixture_unbuilt "$f"; then
    echo "  UNBUILT   $f  — no build/ here; the matcher is NOT verified for it"
    PREFLIGHT_UNBUILT=1
  fi
done
if [[ "$PREFLIGHT_UNBUILT" == "1" ]]; then
  echo "MATCHER PREFLIGHT: NOT VERIFIED — fixture builds are absent (gitignored)."
  echo "  Build them once per checkout:"
  for f in "${EXPECT_PASS[@]}" "${EXPECT_SHORTFALL[@]}"; do
    echo "    npx tsx test/e2e/harness/index.ts setup $f"
  done
  echo "  This is NOT a matcher finding. Exiting 2 so the caller can tell the"
  echo "  difference between 'the check failed' and 'the check did not run'."
  exit 2
fi

for f in "${EXPECT_PASS[@]}"; do
  if timeout 600 npx tsx "$REPO/test/e2e/harness/index.ts" validate "$f" \
      > "/tmp/preflight-$f.txt" 2>&1; then
    echo "  PASS      $f  ($(grep -c 'Overall.*PASS' "/tmp/preflight-$f.txt") pairs)"
  else
    echo "  REGRESSED $f  — expected every pair to pass. See /tmp/preflight-$f.txt"
    grep -E "Overall|Precision|Recall" "/tmp/preflight-$f.txt" | head -8
    PREFLIGHT_BAD=1
  fi
done

for f in "${EXPECT_SHORTFALL[@]}"; do
  if timeout 600 npx tsx "$REPO/test/e2e/harness/index.ts" validate "$f" \
      > "/tmp/preflight-$f.txt" 2>&1; then
    echo "  IMPROVED  $f  — now passes; it was a known shortfall."
    echo "            Update EXPECT_PASS in this file and say what fixed it."
    PREFLIGHT_BAD=1
  else
    echo "  known     $f  ($(grep -c 'Overall.*FAIL' "/tmp/preflight-$f.txt") pairs short — getState/getInitialState stay ambiguous)"
  fi
done

if [[ "$PREFLIGHT_BAD" == "1" ]]; then
  echo "MATCHER PREFLIGHT: OUTCOME SET CHANGED — the matcher behaves differently than recorded."
  # A real exit code: this used to end on an echo, so the one thing the
  # preflight can detect was advisory text an hour before anyone read it.
  exit 1
else
  echo "MATCHER PREFLIGHT: OK — outcome set unchanged"
fi
