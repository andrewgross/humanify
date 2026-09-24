#!/usr/bin/env bash
# WP4.2 builder-input capture: the oracle cut's exact invocation
# (/work/oracle/run-oracle-pair-f7a707d.sh), from the SAME frozen tree
# (f7a707d), with
#   - the capture hook preloaded (wp42-capture-hook.mjs + its loader),
#   - the endpoint DEAD (127.0.0.1:9 — replay only, R14 style),
#   - a scratch COPY of the standing cache (never the standing cache),
#   - every output (tree, logs, dumps) under the scratch gate dir.
# The run's own --dump-artifacts output must reproduce the oracle's
# prompts.jsonl byte-for-byte (the hook is inert), and the cache copy's
# file count must not move (every prompt replayed).
set -u
FROM="$1"; TO="$2"
SRC=/work/oracle-f7a707d-frozen
REPO=/Users/andrewgross/Development/humanify
HERE="$(cd "$(dirname "$0")" && pwd)"
G=/work/rust-port/gates/wp4.2
PAIR="$FROM-$TO"
INPUT="$REPO/../claude-code-versions/inputs/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"
PRIOR="/work/exp050-cold/$FROM-rebased/.humanify/humanified.js"
W=$G/capture/$PAIR
rm -rf "$W"
mkdir -p "$W"
cd "$SRC"
export PATH="$HOME/.bun/bin:$PATH"
export WP42_CAPTURE_DIR="$W/rows" WP42_SRC="$SRC"
NODE_OPTIONS="--max-old-space-size=65536" npx tsx --import "$HERE/wp42-capture-hook.mjs" "$SRC/src/index.ts" "$INPUT" \
  --split \
  --endpoint http://127.0.0.1:9 --model openai/gpt-oss-20b \
  --api-key local --reasoning-effort low -c 32 -o "$W/out" \
  --llm-cache "$G/cache-copy" --prior-version "$PRIOR" \
  --stats-json "$W/stats.json" \
  -vv --log-file "$W/run.log" \
  --diagnostics "$W/diag.json" \
  --dump-artifacts "$W/dumps" \
  > "$W/stdout" 2>&1
RC=$?
echo "$PAIR exit=$RC" >> "$G/capture/progress.txt"
exit $RC
