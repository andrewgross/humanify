#!/usr/bin/env bash
# exp130: score main's shipped defaults (structural) on one index over all 7 corpora, 3 runs at a time.
# usage: score-all.sh <label-stem> <index dir> [log dir]
#   labels: <stem> (dev), <stem>-apps, <stem>-bundlers, pop-/test-/test2-/remin-<stem>
set -uo pipefail
L=$1; IX=$2; LOG=${3:-/work/exp130-data}; C="--index $IX --rule structural --jobs 4"
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
run() { npm run eval -- library "$1" $C --cpus "$2" ${3:+--corpus $3} > "$LOG/score-$1.log" 2>&1; echo "$1 exit $?"; }
run "$L" 0-9 & run "$L-apps" 10-19 apps & run "$L-bundlers" 20-29 bundlers & wait
run "pop-$L" 0-9 pop & run "test-$L" 10-19 test & run "test2-$L" 20-29 test2 & wait
run "remin-$L" 0-9 remin
