#!/usr/bin/env bash
# WP4.1 full-material capture: the oracle cut's exact invocation
# (run-oracle-pair-recut.sh), from the SAME frozen tree (b53b3a8), with
#   - the capture hook preloaded (full typed request per dispatch),
#   - the endpoint DEAD (127.0.0.1:9 — replay only, R14 style),
#   - a scratch COPY of the standing cache (never the standing cache),
#   - every output (tree, logs, dumps) under this scratch dir.
# This is the repo copy of the script that ran from /work/rust-port/gates/
# wp4.1/ on 2026-09-24 (with wp41-capture-hook.mjs there as capture-hook.mjs).
# All four pairs reproduced the oracle's cache-keys.jsonl byte-for-byte
# with the hook loaded, and wrote 0 cache entries.
set -u
FROM="$1"; TO="$2"
SRC=/work/recut-b53b3a8-frozen
REPO=/Users/andrewgross/Development/humanify
G=/work/rust-port/gates/wp4.1
PAIR="$FROM-$TO"
INPUT="$REPO/../claude-code-versions/inputs/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"
PRIOR="/work/exp050-cold/$FROM-rebased/.humanify/humanified.js"
W=$G/capture/$PAIR
mkdir -p "$W"
rm -f "$W/requests.jsonl"
cd "$SRC"
export PATH="$HOME/.bun/bin:$PATH"
export WP41_CAPTURE_OUT="$W/requests.jsonl" WP41_SRC="$SRC"
NODE_OPTIONS="--max-old-space-size=65536" npx tsx --import "$G/capture-hook.mjs" "$SRC/src/index.ts" "$INPUT" \
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
