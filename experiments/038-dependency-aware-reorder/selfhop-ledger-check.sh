#!/usr/bin/env bash
# Self-hop IDEMPOTENCE check, ledger included (what 034's run.sh gates on).
#
# Re-split a version against its own output: same code both sides, so every
# statement is a hash-twin and the pipeline must reproduce its bundle AND its
# split ledger byte-identically. A ledger difference means the split's ASSIGNMENT
# moved — layout data leaking into identity data.
#
#   selfhop-ledger-check.sh <version> [tag]      e.g. 2.1.216 fix
set -uo pipefail
REPO=/Users/andrewgross/Development/humanify
CFG=$REPO/experiments/034-eval-harness/pairs.json
INPUTS=$(jq -r .inputsBase "$CFG")
ENDPOINT=$(jq -r .llm.endpoint "$CFG"); MODEL=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG"); EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")
V=${1:?usage: selfhop-ledger-check.sh <version> [tag]}
TAG=${2:-fix}
# The exp037 sweep names each rebased prior after the TO version it serves.
PRIOR_TREE=${PRIOR_TREE:-/tmp/eval-work/leverb-sweep/$V-rebased}
W=/tmp/eval-work/exp038
run() { NODE_OPTIONS="--max-old-space-size=14336" npx tsx "$REPO/src/index.ts" "$1" \
  --split --endpoint "$ENDPOINT" --model "$MODEL" --api-key "$APIKEY" \
  --reasoning-effort "$EFFORT" -c "$CONC" -o "$2" --llm-cache /tmp/eval-work/llm-cache \
  --prior-version "$3" -vv --log-file "$2.log" > "$2.stdout" 2>&1; }

IN=$INPUTS/claude-code-$V/binary-decompiled/src/entrypoints/index.js
BASE=$W/$V-$TAG; SELF=$W/$V-$TAG-selfhop
echo "=== $V ($TAG): fresh run, prior=$PRIOR_TREE ==="
run "$IN" "$BASE" "$PRIOR_TREE/.humanify/humanified.js"
[ -f "$BASE/.humanify/humanified.js" ] || { echo "RUN FAILED — $BASE.stdout"; exit 1; }
echo "  boot: $( (cd "$BASE" && timeout 60 bun run.cjs --version 2>&1 | tail -1) || true )"
echo "  git churn vs prior: $(git diff --no-index --numstat "$PRIOR_TREE/src" "$BASE/src" 2>/dev/null | awk '$1!="-"&&$2!="-"{a+=$1;d+=$2} END{print a+d}')"

echo "=== $V ($TAG): self-hop against its own output ==="
run "$IN" "$SELF" "$BASE/.humanify/humanified.js"
[ -f "$SELF/.humanify/humanified.js" ] || { echo "SELF-HOP FAILED — $SELF.stdout"; exit 1; }
if cmp -s "$BASE/.humanify/humanified.js" "$SELF/.humanify/humanified.js"; then
  echo "  bundle: IDENTICAL"
else
  echo "  bundle: $(diff "$BASE/.humanify/humanified.js" "$SELF/.humanify/humanified.js" | grep -c '^[<>]') diff lines"
fi
if cmp -s "$BASE/.humanify/split-ledger.json" "$SELF/.humanify/split-ledger.json"; then
  echo "  ledger: IDENTICAL  <- the invariant 034 gates on"
else
  node -e "
    const a=require('$BASE/.humanify/split-ledger.json'), b=require('$SELF/.humanify/split-ledger.json');
    const d=(x,y)=>{let n=0;for(let i=0;i<Math.min(x.length,y.length);i++) if(x[i]!==y[i]) n++; return n;};
    console.log('  ledger: DIFFERS — order', d(a.order,b.order), 'hashes', d(a.hashes||[],b.hashes||[]),
      'aliases', Object.keys(a.aliases||{}).filter(f=>(b.aliases||{})[f]!==a.aliases[f]).length);
  "
fi
echo "  src files differing: $(diff -rq "$BASE/src" "$SELF/src" 2>/dev/null | wc -l | tr -d ' ')"
