#!/usr/bin/env bash
# exp134 LAB: exp132's lab_run.py (a prebuilt, uncommitted binary) over the DEV corpora only (dev, apps, bundlers,
# pop, remin; never test/test2), 3 at a time on 30 cpus starting at <cpu0>. Lab labels are never committed.
# usage: lab.sh <label-stem> <binary> <index dir> <cpu0> [extra run flags...]
set -uo pipefail
L=$1; BIN=$2; IX=$3; C0=$4; shift 4
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
cd "$(dirname "$0")/../../.."
LR=experiments/132-factory-precision/lab_run.py
C="--index $IX --rule structural --jobs 4 $*"
cpus() { echo "$((C0 + $1))-$((C0 + $1 + 9))"; }
run() { $PY -I $LR "$1" --bin "$BIN" $C --cpus "$(cpus "$2")" ${3:+--corpus $3} > "/work/exp134-data/lab-$1.log" 2>&1; echo "$1 exit $?"; }
run "$L" 0 & run "$L-apps" 10 apps & run "pop-$L" 20 pop & wait
run "remin-$L" 0 remin & run "$L-bundlers" 10 bundlers & wait
