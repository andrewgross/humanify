#!/usr/bin/env bash
# exp142: the ONE judge look: main's defaults on the frozen judges (test, test2, remin2), base index then the
# as-of delta index, summaries only (run.py keeps a private corpus's cards and logs out of the repo).
# usage: score-judges.sh <base stem> <base index> <new stem> <new index> <commit> [log dir]
set -uo pipefail
B=$1; BIX=$2; N=$3; NIX=$4; COMMIT=$5; LOG=${6:-/work/exp142-data}
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
run() { npm run eval -- library "$1" --index "$2" --rule structural --jobs 6 --cpus 0-29 --commit "$COMMIT" \
  --corpus "$3" > "$LOG/score-$1.log" 2>&1; echo "$1 exit $?"; }
for c in test test2 remin2; do run "$c-$B" "$BIX" $c; run "$c-$N" "$NIX" $c; done
