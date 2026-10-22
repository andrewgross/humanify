#!/usr/bin/env bash
# exp135 (exp133's script): score one index over all 7 corpora with main's PLAIN library defaults (factory_bodies + exp132's three
# keys on since 4b1d1a49), structural rule, 3 runs at a time (cpus 0-29; the index build holds 34-63).
# EXTRA (optional 3rd+ args after the log dir) are passed to every run, e.g. --pipeline-set library.<key>=v.
# usage: score-all.sh <label-stem> <index dir> [log dir [extra run flags...]]
#   labels: <stem> (dev), <stem>-apps, <stem>-bundlers, pop-/test-/test2-/remin-<stem>
set -uo pipefail
L=$1; IX=$2; LOG=${3:-/work/exp135-data}; shift 3 2>/dev/null || shift $#
C="--index $IX --rule structural --jobs 4 $*"
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
run() { npm run eval -- library "$1" $C --cpus "$2" ${3:+--corpus $3} > "$LOG/score-$1.log" 2>&1; echo "$1 exit $?"; }
run "$L" 0-9 & run "$L-apps" 10-19 apps & run "$L-bundlers" 20-29 bundlers & wait
run "pop-$L" 0-9 pop & run "test-$L" 10-19 test & run "test2-$L" 20-29 test2 & wait
run "remin-$L" 0-9 remin
