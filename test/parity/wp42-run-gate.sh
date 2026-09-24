#!/usr/bin/env bash
# WP4.2 prompt gate over the four oracle pairs (oracle-f7a707d).
#   wp42-run-gate.sh <humanify-binary>            -> dump section only
#   wp42-run-gate.sh <humanify-binary> capture    -> + the captured builder
#     inputs (/work/rust-port/gates/wp4.2/capture/<pair>/rows, written by
#     wp42-capture-pair.sh from the frozen oracle tree)
# Exit 0 only when every pair is IDENTICAL.
set -u
BIN="$1"; MODE="${2:-dump}"
O=/work/oracle/oracle-f7a707d/dumps
C=/work/rust-port/gates/wp4.2/capture
RC=0
for P in 2.1.85-2.1.86 2.1.118-2.1.119 2.1.197-2.1.198 2.1.215-2.1.216; do
  echo "# $P $(date -u +%FT%TZ)"
  if [ "$MODE" = capture ]; then
    "$BIN" prompt-gate "$O/$P" --capture "$C/$P/rows"
  else
    "$BIN" prompt-gate "$O/$P"
  fi
  R=$?
  echo "exit=$R"
  [ $R -ne 0 ] && RC=$R
done
exit $RC
