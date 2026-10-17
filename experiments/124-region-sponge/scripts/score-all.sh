#!/usr/bin/env bash
# exp124: score main's shipped defaults (scorer 9, structural) on one index over every corpus, 3 runs at a time.
# usage: score-all.sh <label-stem> <index dir>     labels: <stem> (dev), <stem>-apps, <stem>-bundlers, pop-/test-/test2-<stem>
set -uo pipefail
L=$1; IX=$2; C="--index $IX --rule structural --jobs 4"
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
run() { npm run eval -- library "$1" $C --cpus "$2" ${3:+--corpus $3} > "/work/exp124-data/score-$1.log" 2>&1; echo "$1 exit $?"; }
run "$L" 0-9 & run "$L-apps" 10-19 apps & run "$L-bundlers" 20-29 bundlers & wait
run "pop-$L" 0-9 pop & run "test-$L" 10-19 test & run "test2-$L" 20-29 test2 & wait
