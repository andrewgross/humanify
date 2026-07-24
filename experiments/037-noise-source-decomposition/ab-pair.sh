#!/usr/bin/env bash
# Same-prior A/B for one version pair: rebase FROM with the current tree,
# then humanify TO twice against that one prior — pass ON (diff-objective)
# vs pass OFF (HUMANIFY_NO_FAMILY_PERMUTE) — and compare noiseLn. Only the
# TO-side pass differs, so the delta is purely the pass. Also self-hops the
# ON output. Usage: ab-pair.sh <FROM> <TO> <HEAP_MB>
set -uo pipefail
FROM=$1; TO=$2; HEAPMB=${3:-14336}
REPO=/Users/andrewgross/Development/humanify-lever1v2
INPUTS=/Users/andrewgross/Development/claude-code-versions/inputs
PRIORS=/Users/andrewgross/Development/unpacked-claude-code/versions
CACHE=/tmp/eval-work/llm-cache
W=/tmp/eval-work/c1-diffobj
IN_FROM=$INPUTS/claude-code-$FROM/binary-decompiled/src/entrypoints/index.js
IN_TO=$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js
ARCH=$PRIORS/claude-code-$FROM/.humanify/humanified.js
REBASE=$W/$FROM-rebased
ON=$W/$TO-on; OFF=$W/$TO-off; SELF=$W/$TO-selfhop
HEAP="--max-old-space-size=$HEAPMB"
mkdir -p "$W"
LLM=(--endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b --api-key local --reasoning-effort low -c 32 --split)
h(){ NODE_OPTIONS="$HEAP" npx tsx "$REPO/src/index.ts" "$1" "${LLM[@]}" -o "$2" --llm-cache "$CACHE" --prior-version "$3" "${@:4}"; }
ledger(){ echo "$1/.humanify/split-ledger.json"; }
hum(){ echo "$1/.humanify/humanified.js"; }

echo "=== [$FROM->$TO] rebase FROM (diffobj) $(date +%H:%M:%S) ==="
rm -rf "$REBASE"; h "$IN_FROM" "$REBASE" "$ARCH" >"$W/$FROM-rebase.log" 2>&1
[ -f "$(hum $REBASE)" ] || { echo "REBASE FAILED"; tail -5 "$W/$FROM-rebase.log"; exit 1; }

echo "=== [$TO] pass ON (diffobj) $(date +%H:%M:%S) ==="
rm -rf "$ON"; h "$IN_TO" "$ON" "$(hum $REBASE)" --stats-json "$ON.stats.json" >"$W/$TO-on.log" 2>&1
[ -f "$(hum $ON)" ] || { echo "ON FAILED"; tail -5 "$W/$TO-on.log"; exit 1; }

echo "=== [$TO] pass OFF $(date +%H:%M:%S) ==="
rm -rf "$OFF"; HUMANIFY_NO_FAMILY_PERMUTE=1 h "$IN_TO" "$OFF" "$(hum $REBASE)" --stats-json "$OFF.stats.json" >"$W/$TO-off.log" 2>&1
[ -f "$(hum $OFF)" ] || { echo "OFF FAILED"; tail -5 "$W/$TO-off.log"; exit 1; }

echo "=== [$TO] self-hop (prior = ON output) $(date +%H:%M:%S) ==="
rm -rf "$SELF"; h "$IN_TO" "$SELF" "$(hum $ON)" >"$W/$TO-selfhop.log" 2>&1
SH="n/a"; [ -f "$(hum $SELF)" ] && { cmp -s "$(hum $ON)" "$(hum $SELF)" && SH=0 || SH=$(diff "$(hum $ON)" "$(hum $SELF)" | grep -c '^[<>]'); }

echo "=== analyze both against the shared prior $(date +%H:%M:%S) ==="
NODE_OPTIONS="$HEAP" npx tsx "$REPO/experiments/034-eval-harness/analyze.ts" "$(hum $ON)"  "$(hum $REBASE)" "$(ledger $ON)"  "$(ledger $REBASE)" "$ON.stats.json"  "$FROM->$TO" >"$W/$TO-on.json"  2>/dev/null
NODE_OPTIONS="$HEAP" npx tsx "$REPO/experiments/034-eval-harness/analyze.ts" "$(hum $OFF)" "$(hum $REBASE)" "$(ledger $OFF)" "$(ledger $REBASE)" "$OFF.stats.json" "$FROM->$TO" >"$W/$TO-off.json" 2>/dev/null

python3 -c "
import json
on=json.load(open('$W/$TO-on.json'))['churn']; off=json.load(open('$W/$TO-off.json'))['churn']
def g(d): return (d['lines']['namingNoiseLines'], d['statements']['unchangedChurned'], d['relocations']['sameNameMovedFile'], d['statements']['novel'], d['lines']['realLines'])
no=g(on); nf=g(off)
labs=['noiseLn','noise','reloc','novel','realLn']
print(f'=========== $FROM->$TO  (self-hop: $SH) ===========')
print(f'{\"KPI\":<10} {\"OFF\":>9} {\"ON\":>9} {\"delta\":>7}')
for i,l in enumerate(labs):
    d=no[i]-nf[i]; print(f'{l:<10} {nf[i]:>9} {no[i]:>9} {d:>+7}  {\"(frozen ok)\" if l in (\"novel\",\"realLn\") and d==0 else (\"REGRESS\" if (l in (\"noiseLn\",\"noise\",\"reloc\") and d>0) or (l in (\"novel\",\"realLn\") and d!=0) else \"\")}')
"
echo "=== DONE $(date +%H:%M:%S) ==="
