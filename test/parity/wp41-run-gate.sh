#!/usr/bin/env bash
# WP4.1 replay gate over the four oracle pairs (oracle-b53b3a8).
#   run-gate.sh <humanify-binary> red   -> requests = the dump's own (lossy) cache-keys.jsonl, 85->86
#   run-gate.sh <humanify-binary> green -> requests = the full-material capture, all four pairs
# The cache is the scratch copy of the entries the four pairs name
# (/tmp/wp41/cache, copied READ-ONLY from /work/neutrality-cache).
set -u
BIN="$1"; MODE="$2"
G=/work/rust-port/gates/wp4.1
O=/work/oracle/oracle-b53b3a8/dumps
CACHE=/tmp/wp41/cache
if [ "$MODE" = red ]; then
  P=2.1.85-2.1.86
  echo "# RED $(date -u +%FT%TZ): requests = the oracle dump's own cache-keys.jsonl ($P)"
  "$BIN" llm-replay-gate "$O/$P/cache-keys.jsonl" "$G/capture/$P/ts-replay.jsonl" "$CACHE" --dump-keys "$O/$P/cache-keys.jsonl" 2>&1 | head -6
  echo "exit=${PIPESTATUS[0]}"
  exit 0
fi
RC=0
for P in 2.1.85-2.1.86 2.1.118-2.1.119 2.1.197-2.1.198 2.1.215-2.1.216; do
  echo "# $P $(date -u +%FT%TZ)"
  "$BIN" llm-replay-gate "$G/capture/$P/requests.jsonl" "$G/capture/$P/ts-replay.jsonl" "$CACHE" --dump-keys "$O/$P/cache-keys.jsonl"
  R=$?
  echo "exit=$R"
  [ $R -ne 0 ] && RC=$R
done
exit $RC
