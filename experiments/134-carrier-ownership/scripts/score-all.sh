#!/usr/bin/env bash
# exp134: score one index over all 7 corpora with main's PLAIN library defaults (4b1d1a49's) plus any extra run
# flags, structural rule, 3 runs at a time on 30 cpus from <cpu0> (two indexes can run side by side: 0 and 30).
# usage: score-all.sh <label-stem> <index dir> <cpu0> [extra run flags...]
#   e.g. score-all.sh exp134-cc /work/exp130-data/full/ix/k2-all-r5-origin 0 \
#          --pipeline-set library.discovery.copy_presence_context=0.02
#   labels: <stem> (dev), <stem>-apps, <stem>-bundlers, pop-/test-/test2-/remin-<stem>; logs in /work/exp134-data
set -uo pipefail
L=$1; IX=$2; C0=$3; shift 3
LOG=/work/exp134-data
C="--index $IX --rule structural --jobs 4 $*"
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
cpus() { echo "$((C0 + $1))-$((C0 + $1 + 9))"; }
run() { npm run eval -- library "$1" $C --cpus "$(cpus "$2")" ${3:+--corpus $3} > "$LOG/score-$1.log" 2>&1; echo "$1 exit $?"; }
run "$L" 0 & run "$L-apps" 10 apps & run "$L-bundlers" 20 bundlers & wait
run "pop-$L" 0 pop & run "test-$L" 10 test & run "test2-$L" 20 test2 & wait
run "remin-$L" 0 remin
