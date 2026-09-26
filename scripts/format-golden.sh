#!/usr/bin/env bash
#
# The `rust:format-golden` check stage: the release binary's formatter
# (`core::format`, pipeline stage 6) replayed against the committed goldens
# — test/parity/format-goldens.json, captured from the TS beautifier before
# the cutover and now the formatter's FROZEN SPEC (test/parity/README.md).
#
# Two runs, because a check that cannot fail proves nothing:
#   1. every golden case must format to its recorded bytes (or error);
#   2. the same check with a planted perturbation must FAIL — proof the
#      comparison is live, not vacuously green.
# Needs target/release/humanify (the rust:build stage builds it).
set -uo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$REPO/target/release/humanify"
GOLDENS="$REPO/test/parity/format-goldens.json"
[[ -x "$BIN" ]] || { echo "no binary at $BIN — the rust:build stage builds it" >&2; exit 1; }

"$BIN" format-check "$GOLDENS" || { echo "FORMAT GOLDENS: the formatter drifted from its frozen spec" >&2; exit 1; }

# The plant must be DETECTED as a differing case — a nonzero exit for any
# other reason (an unknown plant, a crash) is not proof of anything.
PLANTED=$("$BIN" format-check "$GOLDENS" --plant rust-number-format --show 0 2>&1)
if [[ $? -eq 0 || "$PLANTED" != *"differing "[1-9]* ]]; then
  echo "FORMAT GOLDENS: a planted perturbation was NOT detected — the check is vacuous:" >&2
  echo "$PLANTED" | tail -3 >&2
  exit 1
fi
echo "  planted perturbation detected (the check can fail)"
