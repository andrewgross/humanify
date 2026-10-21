#!/usr/bin/env bash
# exp133 LAB: exp132's lab_run.py (a prebuilt, uncommitted binary) over the DEV corpora only (dev, apps, bundlers,
# pop, remin; never test/test2), 3 at a time on cpus 30-59. Lab labels are never committed.
# usage: lab-all.sh <label-stem> <binary> <index dir> [extra run flags...]
set -uo pipefail
L=$1; BIN=$2; IX=$3; shift 3
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
cd "$(dirname "$0")/../../.."
LR=experiments/132-factory-precision/lab_run.py
C="--index $IX --rule structural --jobs 4 $*"
run() { $PY -I $LR "$1" --bin "$BIN" $C --cpus "$2" ${3:+--corpus $3} > "/work/exp133-data/lab-$1.log" 2>&1; echo "$1 exit $?"; }
run "$L" 30-39 & run "$L-apps" 40-49 apps & run "pop-$L" 50-59 pop & wait
run "remin-$L" 30-39 remin & run "$L-bundlers" 40-49 bundlers & wait
