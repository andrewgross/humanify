#!/usr/bin/env bash
# exp142: score main's defaults (structural) with one index over every DEV corpus, ONE label at a time.
# usage: score-all.sh <label-stem> <index dir> <commit> [log dir]
#   labels: <stem> (dev), <stem>-apps, <stem>-bundlers, pop-<stem>, remin-<stem>, remin3-<stem>
set -uo pipefail
L=$1; IX=$2; COMMIT=$3; LOG=${4:-/work/exp142-data}; C="--index $IX --rule structural --jobs 6 --cpus 0-29 --commit $COMMIT"
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
run() { npm run eval -- library "$1" $C ${2:+--corpus $2} > "$LOG/score-$1.log" 2>&1; echo "$1 exit $?"; }
run "$L"; run "$L-apps" apps; run "$L-bundlers" bundlers; run "pop-$L" pop; run "remin-$L" remin; run "remin3-$L" remin3
